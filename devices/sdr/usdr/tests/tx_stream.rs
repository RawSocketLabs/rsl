//! The TX stream's guards on the simulated board: what rsl-usdr refuses before the bus,
//! and what a stopped stream leaves behind. Operations and bytes against libusdr are in
//! `oracle/tests/tx_stream_parity.rs`.

mod common {
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::sim_bus::SimBus;
use num_complex::Complex;
use rsl_usdr::{
    Device, Error, HostFormat, RxConfig, StreamTime, StreamsConfig, SyncMode, TxConfig, TxMute,
    WireFormat,
};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

/// The sync unit's register.
const REG_SYNC: u32 = 14;

/// A TX packet of 0 samples is refused before the bus. Intentional divergence: libusdr
/// takes 0 to mean the transport's largest buffer.
#[test]
fn an_empty_tx_packet_size_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let config = TxConfig::new(0, HostFormat::Ci16, WireFormat::Ci16);
    assert!(matches!(
        untouched(&sim, || device.create_streams(StreamsConfig::tx(config))),
        Err(Error::UnsupportedPacketSize(0))
    ));
}

/// A real host format for TX is refused before the bus. Intentional divergence: libusdr's
/// converter lookup falls back to a byte copy onto the complex wire.
#[test]
fn a_real_tx_host_format_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let config = TxConfig::new(256, HostFormat::F32, WireFormat::Ci16);
    assert!(matches!(
        untouched(&sim, || device.create_streams(StreamsConfig::tx(config))),
        Err(Error::NoConversion {
            host: HostFormat::F32,
            wire: WireFormat::Ci16,
            ..
        })
    ));
}

/// When TX fails to join an RX+TX group created with `SyncMode::All`, the group starts as
/// what it holds, RX alone: the sync unit waits for RX activity (`ST_ADCACT`), not for
/// both (`ST_DELAYADCDAC`), and RX flows.
#[test]
fn a_group_left_with_rx_alone_starts_on_rx_activity() {
    let board = SimBoard::new(BoardRevision::Rev3).with_gateware_build(0xd2b1_0c08);
    let sim = SimBus::new(board);
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let rx = RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16);
    let tx = TxConfig::new(256, HostFormat::Ci16, WireFormat::Ci16);
    let config = StreamsConfig::rx(rx).with_tx(tx).with_sync(SyncMode::All);
    assert!(matches!(
        device.create_streams(config),
        Err(Error::OutdatedGateware(0xd2b1_0c08))
    ));
    let start = sim.board().trace().len();
    device.start_streams().expect("starts what was created");
    let syncs: Vec<u32> = sim.board().trace()[start..]
        .iter()
        .filter_map(|op| match op {
            Op::RegWrite {
                addr: REG_SYNC,
                value,
            } => Some(*value),
            _ => None,
        })
        .collect();
    assert_eq!(
        syncs,
        [1 << 31 | 2 << 16],
        "RX activity, not the duplex start"
    );
    let mut samples = vec![Complex::new(0_i16, 0); 1024];
    device
        .receive_into(&mut [&mut samples], TIMEOUT)
        .expect("RX flows");
}

/// A wait long enough for the sim, which never makes a send wait.
const TIMEOUT: Duration = Duration::from_millis(10);

/// A powered board with a free-running `ci16` TX stream of 256-sample packets, started.
fn streaming() -> (SimBus, Device) {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let config = TxConfig::new(256, HostFormat::Ci16, WireFormat::Ci16);
    device
        .create_streams(StreamsConfig::tx(config))
        .expect("creates");
    device.start_streams().expect("starts");
    (sim, device)
}

/// Runs `call`, asserting it reached nothing on the board, and returns its result.
fn untouched<T>(sim: &SimBus, call: impl FnOnce() -> T) -> T {
    let start = sim.board().trace().len();
    let result = call();
    assert_eq!(
        sim.board().trace().len(),
        start,
        "nothing reached the board"
    );
    result
}

/// Sends the TX core cannot take are refused before the bus: no samples, more than a
/// packet, a time past 47 bits, another host format, another buffer count. The latest
/// 47-bit time reaches the core whole.
#[test]
fn malformed_sends_are_refused_before_the_bus() {
    let (sim, mut device) = streaming();
    let packet = vec![Complex::new(0_i16, 0); 257];
    let floats = vec![Complex::new(0.0_f32, 0.0); 16];
    let late = StreamTime::from_samples(1 << 47);
    let empty: &[Complex<i16>] = &[];
    assert!(matches!(
        untouched(&sim, || device.send(&[empty], None, TIMEOUT)),
        Err(Error::SendSize {
            max: 256,
            got: 0,
            ..
        })
    ));
    assert!(matches!(
        untouched(&sim, || device.send(&[&packet], None, TIMEOUT)),
        Err(Error::SendSize { got: 257, .. })
    ));
    assert!(matches!(
        untouched(&sim, || device.send(&[&packet[..16]], Some(late), TIMEOUT)),
        Err(Error::UnsupportedTimestamp(time)) if time == late
    ));
    assert!(matches!(
        untouched(&sim, || device.send(&[&floats], None, TIMEOUT)),
        Err(Error::HostFormatMismatch { .. })
    ));
    assert!(matches!(
        untouched(&sim, || device.send(
            &[&packet[..16], &packet[..16]],
            None,
            TIMEOUT
        )),
        Err(Error::ChannelBuffers {
            expected: 1,
            got: 2,
            ..
        })
    ));
    let latest = StreamTime::from_samples((1 << 47) - 1);
    device
        .send(&[&packet[..16]], Some(latest), TIMEOUT)
        .expect("the latest 47-bit time is sent");
    assert_eq!(
        sim.board()
            .tx_packets()
            .last()
            .and_then(|played| played.time),
        Some((1 << 47) - 1),
        "every timestamp bit reaches the core"
    );
}

/// Sending and muting need a TX stream: none at all, or an RX-only group.
#[test]
fn sending_or_muting_without_a_tx_stream_is_refused() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let packet = [Complex::new(0_i16, 0); 4];
    assert!(matches!(
        device.send(&[&packet], None, TIMEOUT),
        Err(Error::NotStreaming)
    ));
    device
        .set_rx_sample_rate(20_000_000)
        .expect("sets the rate");
    let rx = RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16);
    device
        .create_streams(StreamsConfig::rx(rx))
        .expect("creates");
    assert!(matches!(
        untouched(&sim, || device.send(&[&packet], None, TIMEOUT)),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        untouched(&sim, || device.set_tx_mute(TxMute::Both)),
        Err(Error::NotStreaming)
    ));
}

/// Gateware with the extended TX core is refused before the bus: its setup is not ported.
#[test]
fn the_extended_tx_core_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3).with_extended_tx_core());
    let mut device = Device::with_bus(sim.clone()).expect("powers up");
    let config = TxConfig::new(256, HostFormat::Ci16, WireFormat::Ci16);
    assert!(matches!(
        untouched(&sim, || device.create_streams(StreamsConfig::tx(config))),
        Err(Error::UnsupportedTxCore)
    ));
}

/// Stopping destroys the group and powers the transmitter down, so the sample rate may
/// change again and a new group can be created.
#[test]
fn a_stopped_tx_group_is_gone() {
    let (sim, mut device) = streaming();
    let packet = [Complex::new(1_i16, -1); 8];
    device.send(&[&packet], None, TIMEOUT).expect("sends");
    device.stop_streams().expect("stops");
    assert!(matches!(device.stop_streams(), Err(Error::NotStreaming)));
    assert_eq!(
        sim.board().lms_reg(0x05) & 1 << 3,
        0,
        "the LMS6002D's transmitter is off (ENCFG STXEN)"
    );
    device
        .set_rx_sample_rate(10_000_000)
        .expect("the rate is free again");
    let config = TxConfig::new(256, HostFormat::Cf32, WireFormat::Ci16);
    device
        .create_streams(StreamsConfig::tx(config))
        .expect("creates again");
}
