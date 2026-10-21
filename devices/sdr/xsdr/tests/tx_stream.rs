//! The TX stream on the simulated XSDR: what it plays and the refusals it makes before any
//! bus access, those libusdr cannot be run on included (it reads past its TX MMCM table).
//! Operation and byte parity with libusdr itself is `oracle/tests/tx_stream_parity.rs`.

mod common;

use std::time::Duration;

use common::sim_bus::SimBus;
use xsdr::{Device, Error, RxChannels, SampleFormat, TxChannels, WireFormat};
use xsdr_sim::{SimBoard, Variant};

/// An XSDR with a TX MMCM in its gateware.
const TX_MMCM: u32 = 0x0030_0020;
/// libusdr's oldest TX gateware (`MINIM_FWID_COMPAT`).
const TX_GATEWARE: u32 = 0xd2b1_0c09;
/// A wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(10);

/// An opened XSDR on `hwid`'s gateware, TX-capable, and the bus to inspect it through.
fn xsdr(hwid: u32) -> (Device, SimBus) {
    let board = SimBoard::new(Variant::Xsdr)
        .with_hwid(hwid)
        .with_gateware_build(TX_GATEWARE);
    let sim = SimBus::new(board);
    let device = Device::open(sim.clone()).expect("an XSDR opens");
    (device, sim)
}

/// The operations the board has traced.
fn traced(sim: &SimBus) -> usize {
    sim.board().trace().len()
}

#[test]
fn both_channels_play_interleaved_sample_by_sample() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    device
        .create_tx_stream(TxChannels::Both, 2)
        .expect("created");
    device.start_tx_stream().expect("started");
    let (a, b) = ([[1, -1], [2, -2]], [[3, -3], [4, -4]]);
    let telemetry = device.send(&[&a, &b], None, TIMEOUT).expect("sent");
    assert_eq!(
        telemetry, None,
        "a buffer the transport starts with has no record"
    );
    let packets = sim.board().tx_packets().to_vec();
    let samples: Vec<i16> = packets[0]
        .bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(
        samples,
        [1, -1, 3, -3, 2, -2, 4, -4],
        "A then B in each sample"
    );
    assert_eq!(packets[0].time, None, "played on arrival");
}

#[test]
fn a_stream_before_any_sample_rate_is_refused_before_any_access() {
    // libusdr sets 1 MS/s, whose 2 MHz link clock its TX MMCM configuration reads past
    // its 64-entry table for.
    let (mut device, sim) = xsdr(TX_MMCM);
    let before = traced(&sim);
    assert!(matches!(
        device.create_tx_stream(TxChannels::A, 1024),
        Err(Error::NoSampleRate)
    ));
    assert_eq!(traced(&sim), before, "nothing reached the bus");
}

#[test]
fn links_the_tx_mmcm_cannot_take_are_refused_before_any_access() {
    // 5 MS/s: a 10 MHz link clock, below the 12.7 MHz the TX MMCM's VCO needs at 63.
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(5_000_000, 5_000_000)
        .expect("RX alone calibrates without it");
    let before = traced(&sim);
    assert!(matches!(
        device.create_tx_stream(TxChannels::A, 1024),
        Err(Error::TxClockOutOfRange)
    ));
    assert_eq!(traced(&sim), before, "nothing reached the bus");

    // With TX streaming at 10 MS/s, a rate down to 5 MS/s is refused likewise.
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    device
        .create_tx_stream(TxChannels::A, 1024)
        .expect("created");
    let before = traced(&sim);
    assert!(matches!(
        device.set_sample_rate(5_000_000, 5_000_000),
        Err(Error::TxClockOutOfRange)
    ));
    assert_eq!(traced(&sim), before, "nothing reached the bus");
    assert!(
        matches!(device.create_rx_stream(RxChannels::A, 1024), Ok(())),
        "the 10 MS/s link still takes an RX stream with TX"
    );
}

#[test]
fn oversized_extended_packets_and_empty_packets_are_refused_before_any_access() {
    // On the extended TX front end (HWID PHY bit 0x08), a packet past one burst, which
    // libusdr splits into bursts.
    let (mut device, sim) = xsdr(TX_MMCM | 0x08);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    let before = traced(&sim);
    assert!(matches!(
        device.create_tx_stream(TxChannels::Both, 15_873),
        Err(Error::UnsupportedPacketSize(15_873))
    ));
    assert_eq!(traced(&sim), before);

    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    let before = traced(&sim);
    assert!(matches!(
        device.create_tx_stream(TxChannels::A, 0),
        Err(Error::UnsupportedPacketSize(0))
    ));
    assert!(matches!(device.start_tx_stream(), Err(Error::NotStreaming)));
    assert!(matches!(device.stop_tx_stream(), Err(Error::NotStreaming)));
    assert_eq!(traced(&sim), before);
}

#[test]
fn sends_unlike_the_stream_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    let sample = [[0_i16; 2]; 8];
    assert!(matches!(
        device.send(&[&sample], None, TIMEOUT),
        Err(Error::NotStreaming)
    ));
    device
        .create_tx_stream(TxChannels::Both, 4)
        .expect("created");
    let before = traced(&sim);
    assert!(matches!(
        device.send(&[&sample[..4]], None, TIMEOUT),
        Err(Error::ChannelBuffers {
            expected: 2,
            got: 1,
            ..
        })
    ));
    assert!(matches!(
        device.send(&[&sample[..4], &sample[..3]], None, TIMEOUT),
        Err(Error::UnequalChannelBuffers {
            first: 4,
            other: 3,
            ..
        })
    ));
    assert!(matches!(
        device.send(&[&sample[..0], &sample[..0]], None, TIMEOUT),
        Err(Error::SendSize { got: 0, .. })
    ));
    assert!(matches!(
        device.send(&[&sample[..4], &sample[..4]], Some(1 << 47), TIMEOUT),
        Err(Error::UnsupportedTimestamp(_))
    ));
    assert_eq!(traced(&sim), before, "nothing reached the bus");
    assert!(
        device
            .send(&[&sample[..4], &sample[..4]], Some((1 << 47) - 1), TIMEOUT)
            .is_ok(),
        "47 bits fit"
    );
}

#[test]
fn tx_formats_the_core_cannot_take_are_refused_before_any_access() {
    let (mut device, sim) = xsdr(TX_MMCM);
    device
        .set_sample_rate(10_000_000, 10_000_000)
        .expect("rate");
    let before = traced(&sim);
    for (wire, host) in [
        // A real wire; a real host, which libusdr byte-copies onto the complex wire.
        (WireFormat::I16, SampleFormat::I16),
        (WireFormat::Ci16, SampleFormat::F32),
        // `ci12` on the original core.
        (WireFormat::Ci12, SampleFormat::Cf32),
    ] {
        assert!(
            matches!(
                device.create_tx_stream_as(TxChannels::A, 1024, wire, host),
                Err(Error::UnsupportedFormat { .. })
            ),
            "{host:?}@{wire:?}"
        );
    }
    assert_eq!(traced(&sim), before, "no access");

    device
        .create_tx_stream_as(TxChannels::A, 4, WireFormat::Ci16, SampleFormat::Cf32)
        .expect("created");
    let before = traced(&sim);
    let ci16 = [[0_i16; 2]; 4];
    assert!(matches!(
        device.send(&[&ci16], None, TIMEOUT),
        Err(Error::HostFormat {
            stream: SampleFormat::Cf32,
            buffers: SampleFormat::Ci16,
            ..
        })
    ));
    assert_eq!(traced(&sim), before, "no access");
}
