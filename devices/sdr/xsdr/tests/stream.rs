//! The RX stream on the simulated XSDR: what it delivers and the refusals it makes before
//! any bus access. Operation and byte parity with libusdr itself is
//! `oracle/tests/stream_parity.rs`.

mod common;

use std::time::Duration;

use common::sim_bus::SimBus;
use xsdr::{Device, Error, RxChannels};
use xsdr_sim::{SimBoard, Variant};

/// An XSDR with a TX MMCM in its gateware.
const TX_MMCM: u32 = 0x0030_0020;
/// An XSDR whose gateware has an RX MMCM too.
const RX_MMCM: u32 = 0x0030_0030;
/// An XSDR with a TX MMCM and a DUC/DDC.
const DUC_DDC: u32 = 0x0030_0024;
/// A receive's wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(10);

/// An opened XSDR on `hwid`'s gateware, and the bus to inspect it through.
fn xsdr(hwid: u32) -> (Device, SimBus) {
    let sim = SimBus::new(SimBoard::new(Variant::Xsdr).with_hwid(hwid));
    let device = Device::open(sim.clone()).expect("an XSDR opens");
    (device, sim)
}

/// The operations the board has traced.
fn traced(sim: &SimBus) -> usize {
    sim.board().trace().len()
}

#[test]
fn both_channels_arrive_split_into_their_own_buffers() {
    let (mut device, _sim) = xsdr(TX_MMCM);
    device
        .create_rx_stream(RxChannels::Both, 4)
        .expect("created");
    device.start_rx_stream().expect("started");
    let (mut a, mut b) = ([[0; 2]; 4], [[0; 2]; 4]);
    let packet = device
        .receive(&mut [&mut a, &mut b], TIMEOUT)
        .expect("a packet");
    assert_eq!((packet.samples, packet.first_sample), (4, 0));
    // The sim's pattern: sample n is (n, -n) on channel A, n + 0x400 and its negation on B.
    assert_eq!(a, [[0, 0], [1, -1], [2, -2], [3, -3]], "channel A");
    assert_eq!(b[1], [0x401, -0x401], "channel B rides lanes 2 and 3");
    let packet = device
        .receive(&mut [&mut a, &mut b], TIMEOUT)
        .expect("a packet");
    assert_eq!(packet.first_sample, 4, "counted from the stream's start");
    assert_eq!(a[0], [4, -4], "the next packet's samples");
}

#[test]
fn nothing_arrives_before_the_start() {
    let (mut device, _sim) = xsdr(TX_MMCM);
    device.create_rx_stream(RxChannels::A, 4).expect("created");
    let mut a = [[0; 2]; 4];
    assert!(matches!(
        device.receive(&mut [&mut a], TIMEOUT),
        Err(Error::Timeout)
    ));
}

#[test]
fn stream_calls_without_a_stream_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(TX_MMCM);
    let before = traced(&sim);
    assert!(matches!(device.start_rx_stream(), Err(Error::NotStreaming)));
    assert!(matches!(
        device.receive(&mut [&mut [[0; 2]; 4]], TIMEOUT),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(device.stop_rx_stream(), Err(Error::NotStreaming)));
    assert_eq!(traced(&sim), before, "no bus access");
}

#[test]
fn receive_buffers_unlike_the_stream_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .create_rx_stream(RxChannels::Both, 4)
        .expect("created");
    device.start_rx_stream().expect("started");
    let before = traced(&sim);
    let mut one = [[0; 2]; 4];
    assert!(matches!(
        device.receive(&mut [&mut one], TIMEOUT),
        Err(Error::ChannelBuffers {
            expected: 2,
            got: 1,
            ..
        })
    ));
    let mut short = [[0; 2]; 3];
    assert!(matches!(
        device.receive(&mut [&mut one, &mut short], TIMEOUT),
        Err(Error::BufferTooSmall {
            needed: 4,
            got: 3,
            ..
        })
    ));
    assert_eq!(traced(&sim), before, "no bus access");
}

#[test]
fn unported_gateware_and_a_second_stream_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(DUC_DDC);
    let before = traced(&sim);
    assert!(matches!(
        device.create_rx_stream(RxChannels::A, 4),
        Err(Error::UnsupportedGateware(0x24))
    ));
    assert_eq!(traced(&sim), before, "no bus access");

    // With an RX MMCM and no rate, libusdr's 1 MS/s default leaves a link its RX-master
    // MMCM configuration reads past its table for.
    let (mut device, sim) = xsdr(RX_MMCM);
    let before = traced(&sim);
    assert!(matches!(
        device.create_rx_stream(RxChannels::A, 4),
        Err(Error::TxClockOutOfRange)
    ));
    assert_eq!(traced(&sim), before, "no bus access");

    let (mut device, sim) = xsdr(TX_MMCM);
    device.create_rx_stream(RxChannels::A, 4).expect("created");
    let before = traced(&sim);
    assert!(matches!(
        device.create_rx_stream(RxChannels::B, 4),
        Err(Error::AlreadyStreaming)
    ));
    assert_eq!(traced(&sim), before, "no bus access");
}
