//! xsdr's RX stream against libusdr's, operation for operation and byte for byte, on fresh
//! simulated XSDRs: everything between the open and the close, the close included.
//!
//! The boards report gateware with a TX MMCM (HWID PHY bit 0x20) unless a scenario says
//! otherwise. The sim's stream engine (`sdr-stream-sim`) delivers a counting pattern once
//! the sync unit releases it; its RX PHY reads an error-free link.
//!
//! libusdr's calls per step: `usdr_dms_create` of `/ll/srx/0` in `ci16`;
//! `usdr_dms_sync(.., "none", ..)` then `USDR_DMS_START`; `usdr_dms_recv`; `USDR_DMS_STOP`
//! then `usdr_dms_destroy`. xsdr's: `create_rx_stream`, `start_rx_stream`, `receive`,
//! `stop_rx_stream`.
//!
//! Whitelisted divergence: a burst padded to whole front-end words arrives without its
//! stub, which libusdr copies into the host buffer with the samples
//! (`bursts_padded_to_whole_words_match_libusdr_up_to_their_stubs`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::time::Duration;

use sim_bus::SimBus;
use xsdr::{Device, Error, RxChannels};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `EBUSY`.
const EBUSY: i32 = 16;
/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;
/// A receive's wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(100);

/// One step of a session, as libusdr's calls and xsdr's method.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/rate/master`: both directions at one rate.
    Rate(u32),
    /// Create the RX stream on these channels with this many samples per packet.
    Create(RxChannels, u32),
    /// Sync free-running, then start.
    Start,
    /// Receive one packet.
    Receive,
    /// Stop, then destroy.
    Stop,
}

/// What a step returned: 0 or a negative errno, and a receive's channels' host bytes, its
/// first sample and its lost-packet count.
type Outcome = (i32, Option<(Vec<Vec<u8>>, u64, u64)>);

/// libusdr's mask for `channels`.
fn mask(channels: RxChannels) -> u64 {
    match channels {
        RxChannels::A => 1,
        RxChannels::B => 2,
        RxChannels::Both => 3,
    }
}

/// libusdr's outcomes and the operations after its open, its close included.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let outcomes = calls
        .iter()
        .map(|&call| {
            let status = |result: Result<(), i32>| (result.err().unwrap_or(0), None);
            match call {
                Call::Rate(rate) => {
                    // SAFETY: the master rate takes its value as a number.
                    status(unsafe { oracle.set_uint(c"/dm/rate/master", rate.into()) })
                }
                Call::Create(channels, samples) => {
                    status(oracle.create_rx_stream(mask(channels), samples))
                }
                Call::Start => status(
                    oracle
                        .sync_rx_stream(c"none")
                        .and_then(|()| oracle.start_rx_stream()),
                ),
                Call::Receive => match oracle.receive_rx(100) {
                    Ok((buffers, info)) => {
                        (0, Some((buffers, info.fsymtime, u64::from(info.totlost))))
                    }
                    Err(errno) => (errno, None),
                },
                Call::Stop => {
                    let stopped = oracle.stop_rx_stream();
                    status(oracle.destroy_rx_stream().and(stopped))
                }
            }
        })
        .collect();
    let board = oracle.close();
    (outcomes, board.trace()[opened..].to_vec())
}

/// xsdr's outcomes, as libusdr's errnos, and its operations after the open, its close
/// included.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    let mut channels = 1;
    let mut samples = 0;
    let outcomes = calls
        .iter()
        .map(|&call| {
            let status = |result: Result<(), Error>| (errno(result), None);
            match call {
                Call::Rate(rate) => status(device.set_sample_rate(rate, rate)),
                Call::Create(on, size) => {
                    channels = if on == RxChannels::Both { 2 } else { 1 };
                    samples = usize::try_from(size).expect("a small packet");
                    status(device.create_rx_stream(on, size))
                }
                Call::Start => status(device.start_rx_stream()),
                Call::Receive => {
                    let mut buffers = vec![vec![[0_i16; 2]; samples]; channels];
                    let mut slices: Vec<&mut [[i16; 2]]> =
                        buffers.iter_mut().map(Vec::as_mut_slice).collect();
                    match device.receive(&mut slices, TIMEOUT) {
                        Ok(packet) => {
                            let bytes = buffers.iter().map(|buffer| host_bytes(buffer)).collect();
                            (0, Some((bytes, packet.first_sample, packet.lost_packets)))
                        }
                        Err(error) => (errno(Err(error)), None),
                    }
                }
                Call::Stop => status(device.stop_rx_stream()),
            }
        })
        .collect();
    device.close().expect("xsdr closes the XSDR");
    let trace = sim.board().trace()[opened..].to_vec();
    (outcomes, trace)
}

/// A channel's samples as libusdr's `ci16` host buffer holds them.
fn host_bytes(samples: &[[i16; 2]]) -> Vec<u8> {
    samples
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// The errno libusdr returns where xsdr returns `result`.
fn errno(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(Error::DoublePumpedMimo | Error::UnsupportedPacketSize(_)) => -EINVAL,
        Err(Error::LinkCalibration) => -ERANGE,
        Err(Error::AlreadyStreaming) => -EBUSY,
        Err(other) => panic!("no libusdr errno for {other:?}"),
    }
}

/// Runs `calls` on fresh boards from `board` through both and compares them whole.
fn compare(board: impl Fn() -> SimBoard, calls: &[Call]) -> Vec<Outcome> {
    compare_up_to(board, calls, usize::MAX)
}

/// As [`compare`], each received buffer compared up to its first `bytes`: libusdr copies
/// a padded burst's stub in with the samples, xsdr leaves it out (whitelisted).
fn compare_up_to(board: impl Fn() -> SimBoard, calls: &[Call], bytes: usize) -> Vec<Outcome> {
    let (mut expected, reference) = libusdr(board(), calls);
    for (_, packet) in &mut expected {
        for buffer in packet.iter_mut().flat_map(|(buffers, ..)| buffers) {
            buffer.truncate(bytes);
        }
    }
    let (actual, trace) = rsl(board(), calls);
    assert_eq!(
        actual, expected,
        "each step's outcome and bytes, as libusdr's"
    );
    assert_eq!(
        trace.len(),
        reference.len(),
        "as many operations as libusdr"
    );
    for (index, (ours, theirs)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(ours, theirs, "operation {index}");
    }
    actual
}

/// A fresh XSDR with a TX MMCM.
fn xsdr() -> SimBoard {
    SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM)
}

/// A session streaming `channels` after a 10 MS/s rate: two packets, then stop.
fn session(channels: RxChannels, samples: u32) -> [Call; 6] {
    [
        Call::Rate(10_000_000),
        Call::Create(channels, samples),
        Call::Start,
        Call::Receive,
        Call::Receive,
        Call::Stop,
    ]
}

#[test]
fn a_stream_on_each_channel_choice_matches_libusdr() {
    for channels in [RxChannels::A, RxChannels::B, RxChannels::Both] {
        let outcomes = compare(xsdr, &session(channels, 1024));
        let second = outcomes[4].1.as_ref().expect("a packet");
        assert_eq!(
            second.1, 1024,
            "{channels:?}: the second packet's first sample"
        );
    }
}

#[test]
fn bursts_padded_to_whole_words_match_libusdr_up_to_their_stubs() {
    // 1023 one-channel samples are 4092 bytes: one burst padded to 4096, whose last 4
    // bytes libusdr hands over as a 1024th sample.
    compare_up_to(xsdr, &session(RxChannels::A, 1023), 4092);
}

#[test]
fn a_packet_split_into_bursts_matches_libusdr() {
    // 16384 samples on both channels: four bursts of 4096.
    compare(xsdr, &session(RxChannels::Both, 16384));
}

#[test]
fn a_stream_before_any_rate_sets_libusdrs_default_rate_first() {
    for channels in [RxChannels::A, RxChannels::Both] {
        compare(
            xsdr,
            &[
                Call::Create(channels, 512),
                Call::Start,
                Call::Receive,
                Call::Stop,
            ],
        );
    }
}

#[test]
fn a_rate_while_streaming_and_a_second_stream_match_libusdr() {
    let outcomes = compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::Create(RxChannels::A, 1024),
            Call::Create(RxChannels::B, 1024),
            Call::Start,
            Call::Rate(20_000_000),
            Call::Receive,
            Call::Stop,
            Call::Create(RxChannels::B, 2048),
            Call::Start,
            Call::Receive,
            Call::Stop,
        ],
    );
    assert_eq!(outcomes[2].0, -EBUSY, "one RX stream at a time");
}

#[test]
fn closing_with_a_stream_running_matches_libusdr() {
    compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::Create(RxChannels::Both, 1024),
            Call::Start,
            Call::Receive,
        ],
    );
}

#[test]
fn a_double_pumped_link_streams_one_channel_like_libusdr() {
    let outcomes = compare(
        xsdr,
        &[
            Call::Rate(80_000_000),
            Call::Create(RxChannels::Both, 1024),
            Call::Create(RxChannels::B, 1024),
            Call::Start,
            Call::Receive,
            Call::Stop,
            Call::Rate(90_000_000),
        ],
    );
    assert_eq!(
        outcomes[1].0, -EINVAL,
        "both channels refused after the map"
    );
}

#[test]
fn a_packet_no_burst_carries_fails_after_the_dma_reset_like_libusdr() {
    let outcomes = compare(
        xsdr,
        &[Call::Rate(10_000_000), Call::Create(RxChannels::A, 0)],
    );
    assert_eq!(outcomes[1].0, -EINVAL);
}

#[test]
fn gateware_without_a_tx_mmcm_fails_the_create_after_streaming_up_like_libusdr() {
    let outcomes = compare(
        || SimBoard::new(Variant::Xsdr),
        &[Call::Rate(10_000_000), Call::Create(RxChannels::A, 1024)],
    );
    assert_eq!(
        outcomes,
        [(-ERANGE, None), (-ERANGE, None)],
        "the rate's calibration fails, then the create's"
    );
}

#[test]
fn gateware_without_a_tx_mmcm_fails_the_create_like_libusdr() {
    let outcomes = compare(
        || SimBoard::new(Variant::Xsdr),
        &[Call::Create(RxChannels::A, 1024)],
    );
    assert_eq!(
        outcomes[0].0, -ERANGE,
        "the default rate's calibration fails"
    );
}

#[test]
fn an_xtrx_streams_on_its_own_bands_like_libusdr() {
    // Streaming up on the automatic path picks the XTRX's first band (its low-band LNA on
    // switch 1), where the XSDR's is the wide band on switch 0.
    for channels in [RxChannels::A, RxChannels::Both] {
        compare(
            || SimBoard::new(Variant::Xtrx).with_hwid(0x002e_0020),
            &session(channels, 1024),
        );
    }
}
