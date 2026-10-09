//! The RX stream's guards on the simulated board: what rsl-usdr refuses, and how it shuts
//! a running stream down.

mod common {
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::sim_bus::SimBus;
use num_complex::Complex;
use rsl_usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use rsl_usdr::{Device, Error};
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
