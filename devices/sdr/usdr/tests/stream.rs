//! The RX stream's guards on the simulated board: what rsl-usdr refuses, and how it shuts
//! a running stream down.

mod common {
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::sim_bus::SimBus;
use num_complex::Complex;
use rsl_usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use rsl_usdr::{Device, Error, HostFormat, RxChannels, RxConfig, StreamsConfig, WireFormat};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// The RX DMA control register.
const REG_RX_DMA_CONTROL: u32 = 5;
/// FPGA register latching GPO banks.
const REG_GPO: u32 = 0;

/// A powered board, tuned and streaming 1024-sample packets.
fn streaming() -> (SimBus, Device) {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.set_rx_frequency(1_000_000_000).expect("tunes");
    device.start_rx_stream(1024).expect("starts streaming");
    (sim, device)
}

/// Closing a streaming device stops the DMA before the board loses power.
#[test]
fn closing_while_streaming_stops_the_stream_before_powering_down() {
    let (sim, device) = streaming();
    let start = sim.board().trace().len();
    device.close().expect("closes");
    let trace = sim.board().trace()[start..].to_vec();
    let dma_off = trace
        .iter()
        .position(|op| {
            *op == Op::RegWrite {
                addr: REG_RX_DMA_CONTROL,
                value: 0,
            }
        })
        .expect("the DMA is stopped");
    let power_down = trace
        .iter()
        .position(|op| matches!(op, Op::RegWrite { addr: REG_GPO, .. }))
        .expect("the board powers down");
    assert!(
        dma_off < power_down,
        "DMA off at {dma_off}, power down at {power_down}"
    );
}

/// The stream's block size and decimation are fixed while it runs.
#[test]
fn the_sample_rate_cannot_change_while_streaming() {
    let (sim, mut device) = streaming();
    let start = sim.board().trace().len();
    assert!(matches!(
        device.set_rx_sample_rate(10_000_000),
        Err(Error::AlreadyStreaming)
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// A buffer smaller than a packet is refused before the bus is touched.
#[test]
fn a_buffer_smaller_than_a_packet_is_refused() {
    let (sim, mut device) = streaming();
    let start = sim.board().trace().len();
    let mut samples = vec![Complex::new(0, 0); 1023];
    let result = device.receive(&mut samples, Duration::from_millis(10));
    assert!(
        matches!(
            result,
            Err(Error::BufferTooSmall {
                needed: 1024,
                got: 1023,
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// Stream calls need a stream.
#[test]
fn receiving_or_stopping_without_a_stream_is_refused() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim).expect("powers up");
    let mut samples = vec![Complex::new(0, 0); 1024];
    assert!(matches!(
        device.receive(&mut samples, Duration::from_millis(10)),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(device.stop_rx_stream(), Err(Error::NotStreaming)));
    assert!(matches!(device.set_rx_ready(), Err(Error::NotStreaming)));
    assert_eq!(device.rx_packet_samples(), None);
}

/// [`SimBus`] for control, with RX blocks served from a queue: each a byte value to fill
/// the block with and the lost-packet count its out-of-band word carries.
struct ServingBus {
    /// The simulated board, for everything but the stream data.
    sim: SimBus,

    /// Blocks to deliver, in order.
    blocks: Vec<(u8, u64)>,

    /// The block size the stream asked for.
    block_bytes: usize,
}

impl Bus for ServingBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.sim.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.sim.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.sim.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.sim.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.sim.sleep(duration);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.block_bytes = usize::try_from(block_bytes).expect("fits");
        Ok(())
    }

    fn rx_stream_recv(
        &mut self,
        _timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        if self.blocks.is_empty() {
            return Err(BusError::Timeout);
        }
        let (fill, lost) = self.blocks.remove(0);
        consume(&vec![fill; self.block_bytes], [lost, 0]);
        Ok(())
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        Ok(())
    }
}

/// Lost packets advance the sample count past the gap and add up, as libusdr counts them
/// from the gateware's out-of-band word (bits 23:0; the burst mask above is ignored).
#[test]
fn lost_packets_advance_the_sample_count_and_accumulate() {
    let bus = ServingBus {
        sim: SimBus::new(SimBoard::new(BoardRevision::Rev3)),
        blocks: vec![(1, 0), (2, 0x8000_0000_0000_0003), (3, 0x0100_0002)],
        block_bytes: 0,
    };
    let mut device = Device::with_bus(bus).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    device.start_rx_stream(1024).expect("starts streaming");
    let mut samples = vec![Complex::new(0, 0); 1024];
    let mut receive = || {
        device
            .receive(&mut samples, Duration::from_millis(10))
            .expect("a packet")
    };
    let packets = [receive(), receive(), receive()];
    let summary: Vec<(u64, u64)> = packets
        .iter()
        .map(|packet| (packet.first_sample, packet.lost_packets))
        .collect();
    // 1024 samples per packet: 3 lost before the second, 2 more before the third.
    assert_eq!(summary, [(0, 0), (4 * 1024, 3), (7 * 1024, 5)]);
}

/// A packet that cannot split into whole-word bursts arrives padded; the samples come out
/// continuous, the stubs skipped (libusdr's pass-through copy would include them).
#[test]
fn a_padded_packet_arrives_without_its_stubs() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    // 16383 samples: three bursts of 5461, each ending in a 4-byte stub.
    device.start_rx_stream(16383).expect("starts streaming");
    let mut samples = vec![Complex::new(0, 0); 16383];
    device
        .receive(&mut samples, Duration::from_millis(10))
        .expect("a packet");
    let expected: Vec<Complex<i16>> = (0..16383_u16)
        .map(|n| {
            let i = i16::from_le_bytes(n.to_le_bytes());
            Complex::new(i, i.wrapping_neg())
        })
        .collect();
    assert_eq!(samples, expected, "the sim's counting pattern, unbroken");
}

/// A ci12-wire, cf32-host stream on the sim, created but not started.
fn created() -> (SimBus, Device) {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let config = RxConfig::new(1024, HostFormat::Cf32, WireFormat::Ci12);
    device
        .create_streams(StreamsConfig::rx(config))
        .expect("creates");
    (sim, device)
}

/// Buffers that cannot take the stream's packet are refused before the bus is touched:
/// another host format, or a buffer count other than the stream's one channel.
#[test]
fn mismatched_receive_buffers_are_refused_before_the_bus() {
    let (sim, mut device) = created();
    device.start_streams().expect("starts");
    let start = sim.board().trace().len();
    let mut ci16 = vec![Complex::new(0_i16, 0); 1024];
    assert!(matches!(
        device.receive_into(&mut [&mut ci16], Duration::from_millis(10)),
        Err(Error::HostFormatMismatch {
            stream: HostFormat::Cf32,
            buffer: HostFormat::Ci16,
            ..
        })
    ));
    let (mut first, mut second) = (
        vec![Complex::new(0.0_f32, 0.0); 1024],
        vec![Complex::new(0.0_f32, 0.0); 1024],
    );
    assert!(matches!(
        device.receive_into(&mut [&mut first, &mut second], Duration::from_millis(10)),
        Err(Error::ChannelBuffers {
            expected: 1,
            got: 2,
            ..
        })
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
    device
        .receive_into(&mut [&mut first], Duration::from_millis(10))
        .expect("the stream's own format is received");
}

/// Starting needs a created group.
#[test]
fn starting_without_a_group_is_refused() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim).expect("powers up");
    assert!(matches!(device.start_streams(), Err(Error::NotStreaming)));
}

/// A configuration with no stream creates nothing and touches nothing.
#[test]
fn a_configuration_without_streams_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let mut empty = StreamsConfig::rx(RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16));
    empty.rx = None;
    let start = sim.board().trace().len();
    assert!(matches!(
        device.create_streams(empty),
        Err(Error::NoStreams)
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// A started group cannot start again, and writes nothing trying.
#[test]
fn a_group_starts_only_once() {
    let (sim, mut device) = created();
    device.start_streams().expect("starts");
    let start = sim.board().trace().len();
    assert!(matches!(
        device.start_streams(),
        Err(Error::AlreadyStreaming)
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// Stopping destroys the group, so a second stop has nothing to stop.
#[test]
fn a_stopped_group_is_gone() {
    let (_, mut device) = created();
    device.stop_streams().expect("stops");
    assert!(matches!(device.stop_streams(), Err(Error::NotStreaming)));
}

/// A created group delivers nothing until started: the sync unit holds it.
#[test]
fn a_created_group_waits_for_its_start() {
    let (_, mut device) = created();
    let mut samples = vec![Complex::new(0.0_f32, 0.0); 1024];
    assert!(matches!(
        device.receive_into(&mut [&mut samples], Duration::from_millis(10)),
        Err(Error::Timeout)
    ));
    device.start_streams().expect("starts");
    let metadata = device
        .receive_into(&mut [&mut samples], Duration::from_millis(10))
        .expect("a packet");
    assert_eq!(
        metadata.time.samples(),
        0,
        "the first packet starts the count"
    );
    // The sim's 12-bit pattern: sample 1 is I 1, Q -1, unpacked to the top of 16 bits
    // (±16), then scaled by libusdr's 1/32767.
    let scale = 1.0_f32 / 32767.0;
    assert_eq!(
        samples[1],
        Complex::new(16.0 * scale, -16.0 * scale),
        "sample 1 of the 12-bit counting pattern"
    );
}

/// Stream configurations rsl-usdr refuses before any bus access where libusdr would go
/// on (whitelisted divergences, `PORTING.md`'s formats row): complex channel 1, which
/// would be lanes 2 and 3, and a real 12-bit packet whose burst ends mid-byte (1023
/// samples of 12 bits), which libusdr converts whole, stub included.
#[test]
fn rx_streams_the_front_end_cannot_carry_are_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let start = sim.board().trace().len();
    let complex_channel_1 =
        RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16).with_channels(RxChannels::Channel1);
    assert!(matches!(
        device.create_streams(StreamsConfig::rx(complex_channel_1)),
        Err(Error::UnsupportedRxChannels {
            channels: RxChannels::Channel1,
            wire: WireFormat::Ci16,
            ..
        })
    ));
    let half_byte = RxConfig::new(1023, HostFormat::I16, WireFormat::I12);
    assert!(matches!(
        device.create_streams(StreamsConfig::rx(half_byte)),
        Err(Error::UnsupportedPacketSize(1023))
    ));
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
}

/// A real stream on channel 1 delivers the receiver's Q lane alone: the sim's counting
/// pattern's Q is minus the sample number.
#[test]
fn a_real_channel_1_stream_delivers_the_q_lane() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let config =
        RxConfig::new(1024, HostFormat::I16, WireFormat::I16).with_channels(RxChannels::Channel1);
    device
        .create_streams(StreamsConfig::rx(config))
        .expect("creates");
    device.start_streams().expect("starts");
    let mut samples = vec![0_i16; 1024];
    device
        .receive_into(&mut [&mut samples], Duration::from_millis(10))
        .expect("a packet");
    assert_eq!(
        samples[..3],
        [0, -1, -2],
        "the Q lane of the counting pattern"
    );
}
