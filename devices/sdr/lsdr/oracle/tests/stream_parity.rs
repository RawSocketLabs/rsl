//! lsdr's RX stream create against libusdr's, operation for operation and outcome for
//! outcome, on fresh simulated boards: every channel set and several packet sizes, from
//! the open to the close inclusive.
//!
//! At the pinned libusdr the create fails once the RX front end and DMA are configured:
//! the `LSDr` declares no `/ll/sync/0/base`, so holding the sync unit stopped fails with
//! `-ENOENT` (`sfetrx4_stream_sync`). lsdr fails there too (`Error::NoSyncUnit`).
//!
//! Whitelisted divergence, none: no scenario here injects a bus fault, where lsdr reports
//! a failed ADC power-up libusdr ignores (`PORTING.md`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use lsdr::{Device, Error, OpenOptions, RxChannels};
use lsdr_oracle::Oracle;
use lsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `ENOENT`: the missing sync unit's path.
const ENOENT: i32 = 2;
/// `EINVAL`: a packet size the front end cannot split.
const EINVAL: i32 = 22;

/// libusdr's channel mask for `channels` (`usdr_dms_create_ex`).
fn mask(channels: RxChannels) -> u64 {
    match channels {
        RxChannels::A => 1,
        RxChannels::B => 2,
        RxChannels::Both => 3,
        other => panic!("no libusdr mask for {other:?}"),
    }
}

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: lsdr diverges from libusdr at op {at}: libusdr {}, lsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// Opens `variant` in both drivers, sets `rate` first if given, creates the stream and
/// closes; compares the whole sessions and returns libusdr's create outcome.
fn assert_same_create(
    variant: Variant,
    rate: Option<u32>,
    channels: RxChannels,
    samples: u32,
) -> Result<(), i32> {
    let scenario = format!("{variant:?}, {rate:?}, {channels:?}, {samples} samples");
    let mut oracle = Oracle::open(SimBoard::new(variant), c"").expect("libusdr opens the board");
    if let Some(rate) = rate {
        oracle.set_sample_rate(rate).expect("libusdr sets the rate");
    }
    let expected = oracle.create_rx_stream(mask(channels), samples);
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(variant));
    let mut device = Device::open(sim.clone(), OpenOptions::new()).expect("lsdr opens");
    if let Some(rate) = rate {
        device.set_sample_rate(rate).expect("lsdr sets the rate");
    }
    let actual = device
        .create_rx_stream(channels, samples)
        .map_err(|error| match error {
            Error::NoSyncUnit => -ENOENT,
            Error::UnsupportedPacketSize(_) => -EINVAL,
            other => panic!("{scenario}: unexpected {other:?}"),
        });
    device.close().expect("lsdr closes the board");

    assert_same_ops(reference.trace(), sim.board().trace(), &scenario);
    assert_eq!(expected, actual, "{scenario}: outcomes differ");
    expected
}

#[test]
fn every_channel_set_is_configured_then_refused_at_the_sync_unit_like_libusdr() {
    for variant in [Variant::SingleAdc, Variant::DualAdcClockGen] {
        for channels in [RxChannels::A, RxChannels::B, RxChannels::Both] {
            for samples in [4096, 1000, 65_536] {
                assert_eq!(
                    assert_same_create(variant, None, channels, samples),
                    Err(-ENOENT)
                );
            }
        }
    }
}

#[test]
fn a_stream_after_a_rate_matches_libusdr() {
    assert_eq!(
        assert_same_create(Variant::SingleAdc, Some(50_000_000), RxChannels::Both, 8192),
        Err(-ENOENT)
    );
}

#[test]
fn packet_sizes_the_front_end_cannot_split_are_refused_after_the_dma_reset_like_libusdr() {
    for samples in [0, 1 << 24] {
        assert_eq!(
            assert_same_create(Variant::SingleAdc, None, RxChannels::A, samples),
            Err(-EINVAL)
        );
    }
}
