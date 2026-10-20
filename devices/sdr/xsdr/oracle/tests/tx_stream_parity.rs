//! xsdr's TX stream against libusdr's, operation for operation and byte for byte, on fresh
//! simulated XSDRs: everything between the open and the close, the close included, every
//! send's statistics and every packet the TX core played.
//!
//! The boards report gateware with a TX MMCM (HWID PHY bit 0x20) stamped as new as
//! libusdr's TX minimum unless a scenario says otherwise. The sim's TX MMCM locks at once
//! and its RX PHY reads an error-free link unless the scenario scripts its reads; its TX
//! core plays each packet as soon as it runs and the sync unit lets it.
//!
//! libusdr's calls per step: `usdr_dms_create` of `/ll/stx/0` (or `/ll/srx/0`) in `ci16`;
//! `USDR_DMS_START` then `usdr_dms_sync(.., "none", ..)` for TX (sync, then start, for RX);
//! `usdr_dms_send_stat`; `usdr_dms_recv`; `USDR_DMS_STOP` then `usdr_dms_destroy`.
//! xsdr's: `create_tx_stream`, `start_tx_stream`, `send`, `stop_tx_stream`, and the RX
//! stream's own.
//!
//! Not run against libusdr, which reads past its MMCM table there: a TX stream before any
//! sample rate (libusdr's 1 MS/s default) or at a TX rate below about 6.35 MS/s; xsdr
//! refuses both before any bus access (`tests/tx_stream.rs`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::time::Duration;

use sim_bus::SimBus;
use xsdr::{Device, Error, RxChannels, TxChannels, TxTelemetry};
use xsdr_oracle::{Oracle, SendStat};
use xsdr_sim::{Op, SimBoard, TxPacket, Variant};

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `EBUSY`.
const EBUSY: i32 = 16;
/// `ETIMEDOUT`.
const ETIMEDOUT: i32 = 110;
/// `ECONNRESET`: gateware too old for TX.
const ECONNRESET: i32 = 104;
/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;
/// libusdr's oldest TX gateware (`MINIM_FWID_COMPAT`).
const TX_GATEWARE: u32 = 0xd2b1_0c09;
/// A wait; the sim answers at once.
const TIMEOUT: Duration = Duration::from_millis(100);

/// One step of a session.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/rate/master`: both directions at one rate.
    Rate(u32),
    /// Create the RX stream.
    CreateRx(RxChannels, u32),
    /// Sync free-running, then start RX.
    StartRx,
    /// Receive one RX packet.
    Receive,
    /// Stop and destroy RX.
    StopRx,
    /// Create the TX stream.
    CreateTx(TxChannels, u32),
    /// Start TX, then sync free-running.
    StartTx,
    /// Send this many samples per channel, at this time or now.
    Send(u32, Option<u64>),
    /// Stop and destroy TX.
    StopTx,
}

/// What a step returned: 0 or a negative errno, and a send's statistics or a receive's
/// first sample.
type Outcome = (i32, Option<(u64, u64, u32, u64)>);

/// libusdr's mask for channels A, B or both.
const fn mask(a: bool, b: bool) -> u64 {
    (a as u64) | (b as u64) << 1
}

/// The RX mask.
fn rx_mask(channels: RxChannels) -> u64 {
    match channels {
        RxChannels::A => mask(true, false),
        RxChannels::B => mask(false, true),
        RxChannels::Both => mask(true, true),
    }
}

/// The TX mask and channel count.
fn tx_mask(channels: TxChannels) -> (u64, usize) {
    match channels {
        TxChannels::A => (mask(true, false), 1),
        TxChannels::B => (mask(false, true), 1),
        TxChannels::Both => (mask(true, true), 2),
    }
}

/// The samples a send carries on `channel`: `I = 100 * channel + n`, `Q = -I`, from the
/// send's index `send`.
fn samples(send: usize, channel: usize, count: u32) -> Vec<[i16; 2]> {
    (0..count)
        .map(|n| {
            let value =
                i16::try_from((send * 1000 + channel * 100 + n as usize) % 30_000).expect("small");
            [value, -value]
        })
        .collect()
}

/// A channel's samples as `ci16` host bytes.
fn host_bytes(samples: &[[i16; 2]]) -> Vec<u8> {
    samples
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// libusdr's outcomes, operations after its open (its close included) and played
/// packets.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>, Vec<TxPacket>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let mut tx_channels = 1;
    let outcomes = calls
        .iter()
        .enumerate()
        .map(|(index, &call)| {
            let status = |result: Result<(), i32>| (result.err().unwrap_or(0), None);
            match call {
                Call::Rate(rate) => {
                    // SAFETY: the master rate takes its value as a number.
                    status(unsafe { oracle.set_uint(c"/dm/rate/master", rate.into()) })
                }
                Call::CreateRx(channels, size) => {
                    status(oracle.create_rx_stream(rx_mask(channels), size))
                }
                Call::StartRx => status(
                    oracle
                        .sync_rx_stream(c"none")
                        .and_then(|()| oracle.start_rx_stream()),
                ),
                Call::Receive => match oracle.receive_rx(100) {
                    Ok((_, info)) => (0, Some((info.fsymtime, u64::from(info.totlost), 0, 0))),
                    Err(errno) => (errno, None),
                },
                Call::StopRx => {
                    let stopped = oracle.stop_rx_stream();
                    status(oracle.destroy_rx_stream().and(stopped))
                }
                Call::CreateTx(channels, size) => {
                    let (mask, count) = tx_mask(channels);
                    let result = oracle.create_tx_stream(mask, size);
                    if result.is_ok() {
                        tx_channels = count;
                    }
                    status(result)
                }
                Call::StartTx => status(
                    oracle
                        .start_tx_stream()
                        .and_then(|()| oracle.sync_tx_stream(c"none")),
                ),
                Call::Send(count, time) => {
                    let buffers: Vec<Vec<u8>> = (0..tx_channels)
                        .map(|channel| host_bytes(&samples(index, channel, count)))
                        .collect();
                    let slices: Vec<&[u8]> = buffers.iter().map(Vec::as_slice).collect();
                    match oracle.send_tx(&slices, count, time.unwrap_or(u64::MAX), 100) {
                        Ok(stat) => (0, stat.map(stat_of)),
                        Err(errno) => (errno, None),
                    }
                }
                Call::StopTx => {
                    let stopped = oracle.stop_tx_stream();
                    status(oracle.destroy_tx_stream().and(stopped))
                }
            }
        })
        .collect();
    let board = oracle.close();
    let trace = board.trace()[opened..].to_vec();
    (outcomes, trace, board.tx_packets().to_vec())
}

/// A send's statistics as both report them.
fn stat_of(stat: SendStat) -> (u64, u64, u32, u64) {
    (
        stat.lhwtime,
        u64::from(stat.ktime),
        stat.underruns,
        u64::from(stat.fifo_used),
    )
}

/// xsdr's telemetry as libusdr's statistics.
fn telemetry_of(telemetry: TxTelemetry) -> (u64, u64, u32, u64) {
    (
        u64::from(telemetry.hardware_time),
        u64::from(telemetry.kernel_time),
        u32::try_from(telemetry.underruns).expect("few"),
        u64::from(telemetry.fifo_used),
    )
}

/// xsdr's outcomes, as libusdr's errnos, its operations after the open (its close
/// included) and the packets played.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>, Vec<TxPacket>) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    let (mut rx_channels, mut rx_samples, mut tx_channels) = (1, 0, 1);
    let outcomes = calls
        .iter()
        .enumerate()
        .map(|(index, &call)| {
            let status = |result: Result<(), Error>| (errno(result), None);
            match call {
                Call::Rate(rate) => status(device.set_sample_rate(rate, rate)),
                Call::CreateRx(on, size) => {
                    rx_channels = if on == RxChannels::Both { 2 } else { 1 };
                    rx_samples = usize::try_from(size).expect("a small packet");
                    status(device.create_rx_stream(on, size))
                }
                Call::StartRx => status(device.start_rx_stream()),
                Call::Receive => {
                    let mut buffers = vec![vec![[0_i16; 2]; rx_samples]; rx_channels];
                    let mut slices: Vec<&mut [[i16; 2]]> =
                        buffers.iter_mut().map(Vec::as_mut_slice).collect();
                    match device.receive(&mut slices, TIMEOUT) {
                        Ok(packet) => (0, Some((packet.first_sample, packet.lost_packets, 0, 0))),
                        Err(error) => (errno(Err(error)), None),
                    }
                }
                Call::StopRx => status(device.stop_rx_stream()),
                Call::CreateTx(channels, size) => {
                    let result = device.create_tx_stream(channels, size);
                    if result.is_ok() {
                        tx_channels = tx_mask(channels).1;
                    }
                    status(result)
                }
                Call::StartTx => status(device.start_tx_stream()),
                Call::Send(count, time) => {
                    let buffers: Vec<Vec<[i16; 2]>> = (0..tx_channels)
                        .map(|channel| samples(index, channel, count))
                        .collect();
                    let slices: Vec<&[[i16; 2]]> = buffers.iter().map(Vec::as_slice).collect();
                    match device.send(&slices, time, TIMEOUT) {
                        Ok(telemetry) => (0, telemetry.map(telemetry_of)),
                        Err(error) => (errno(Err(error)), None),
                    }
                }
                Call::StopTx => status(device.stop_tx_stream()),
            }
        })
        .collect();
    device.close().expect("xsdr closes the XSDR");
    let board = sim.board();
    let trace = board.trace()[opened..].to_vec();
    (outcomes, trace, board.tx_packets().to_vec())
}

/// The errno libusdr returns where xsdr returns `result`.
fn errno(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(Error::DoublePumpedMimo | Error::UnsupportedPacketSize(_)) => -EINVAL,
        Err(Error::LinkCalibration) => -ERANGE,
        Err(Error::AlreadyStreaming) => -EBUSY,
        Err(Error::Timeout) => -ETIMEDOUT,
        Err(Error::OutdatedGateware(_)) => -ECONNRESET,
        Err(other) => panic!("no libusdr errno for {other:?}"),
    }
}

/// Runs `calls` on fresh boards from `board` through both and compares them whole: each
/// step's outcome, every operation and every packet played.
fn compare(board: impl Fn() -> SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let (expected, reference, played) = libusdr(board(), calls);
    let (actual, trace, ours) = rsl(board(), calls);
    assert_eq!(actual, expected, "each step's outcome, as libusdr's");
    for (index, (ours, theirs)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(ours, theirs, "operation {index}");
    }
    assert_eq!(
        trace.len(),
        reference.len(),
        "as many operations as libusdr"
    );
    assert_eq!(ours, played, "the packets the TX core played");
    (actual, trace)
}

/// A fresh XSDR with a TX MMCM and gateware libusdr streams TX from.
fn xsdr() -> SimBoard {
    SimBoard::new(Variant::Xsdr)
        .with_hwid(TX_MMCM)
        .with_gateware_build(TX_GATEWARE)
}

/// How many times `trace` writes `value` to FPGA register `addr`.
fn writes(trace: &[Op], addr: u32, value: u32) -> usize {
    trace
        .iter()
        .filter(|op| **op == Op::RegWrite { addr, value })
        .count()
}

/// The TX PHY port's write holding the TX MMCM in reset, TX master: one per
/// configuration.
const MMCM_HELD: u32 = 2 << 24 | 5;

/// Whether `trace` holds the LMS7002M write `word` (address and value, write bit set).
fn wrote_spi(trace: &[Op], word: u32) -> bool {
    trace
        .iter()
        .any(|op| matches!(op, Op::Spi { out, .. } if *out == word))
}

#[test]
fn a_stream_on_each_channel_choice_matches_libusdr() {
    for channels in [TxChannels::A, TxChannels::B, TxChannels::Both] {
        let (outcomes, _) = compare(
            xsdr,
            &[
                Call::Rate(10_000_000),
                Call::CreateTx(channels, 1024),
                Call::StartTx,
                Call::Send(1024, None),
                Call::Send(512, Some(0x1234_5678_9abc)),
                Call::StopTx,
            ],
        );
        assert!(
            outcomes.iter().all(|(status, _)| *status == 0),
            "{channels:?}: every step succeeds"
        );
    }
}

#[test]
fn sends_before_the_start_queue_until_the_buffers_run_out_like_libusdr() {
    let mut calls = vec![Call::Rate(10_000_000), Call::CreateTx(TxChannels::A, 64)];
    calls.extend([Call::Send(64, None); 33]);
    calls.extend([Call::StartTx, Call::Send(64, None), Call::StopTx]);
    let (outcomes, _) = compare(xsdr, &calls);
    assert_eq!(outcomes[34].0, -ETIMEDOUT, "the 33rd send finds no buffer");
    assert!(
        outcomes[36].1.is_some(),
        "a buffer a played packet freed carries its record"
    );
}

#[test]
fn rx_and_tx_together_match_libusdr_in_either_order() {
    compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateRx(RxChannels::A, 1024),
            Call::StartRx,
            Call::CreateTx(TxChannels::Both, 512),
            Call::StartTx,
            Call::Send(512, None),
            Call::Receive,
            Call::StopTx,
            Call::Receive,
            Call::StopRx,
        ],
    );
    compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateTx(TxChannels::A, 1024),
            Call::CreateRx(RxChannels::Both, 1024),
            Call::StartTx,
            Call::StartRx,
            Call::Send(1024, None),
            Call::Receive,
            Call::StopRx,
            Call::StopTx,
        ],
    );
}

#[test]
fn a_second_stream_and_a_recreate_match_libusdr() {
    let (outcomes, _) = compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateTx(TxChannels::A, 1024),
            Call::CreateTx(TxChannels::B, 1024),
            Call::StopTx,
            Call::CreateTx(TxChannels::B, 2048),
            Call::StartTx,
            Call::Send(2048, None),
            Call::StopTx,
        ],
    );
    assert_eq!(outcomes[2].0, -EBUSY, "one TX stream at a time");
}

#[test]
fn closing_with_a_stream_running_matches_libusdr() {
    compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateRx(RxChannels::B, 1024),
            Call::CreateTx(TxChannels::Both, 1024),
            Call::StartTx,
            Call::Send(100, None),
        ],
    );
}

#[test]
fn a_double_pumped_link_transmits_one_channel_like_libusdr() {
    let (outcomes, _) = compare(
        xsdr,
        &[
            Call::Rate(80_000_000),
            Call::CreateTx(TxChannels::Both, 1024),
            Call::CreateTx(TxChannels::B, 1024),
            Call::StartTx,
            Call::Send(1024, None),
            Call::StopTx,
        ],
    );
    assert_eq!(
        outcomes[1].0, -EINVAL,
        "both channels refused after the map"
    );
}

#[test]
fn outdated_gateware_and_an_oversized_packet_fail_after_the_calibration_like_libusdr() {
    let (outcomes, _) = compare(
        || SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM),
        &[Call::Rate(10_000_000), Call::CreateTx(TxChannels::A, 1024)],
    );
    assert_eq!(outcomes[1].0, -ECONNRESET, "USR_ACCESS2 reads 0");
    // 16 000 samples on both channels: 128 000 bytes, past the core's 126 976.
    let (outcomes, _) = compare(
        xsdr,
        &[
            Call::Rate(10_000_000),
            Call::CreateTx(TxChannels::Both, 16_000),
            Call::CreateTx(TxChannels::Both, 15_872),
            Call::StopTx,
        ],
    );
    assert_eq!(outcomes[1].0, -EINVAL, "after the sync unit is stopped");
    assert_eq!(outcomes[2].0, 0, "126 976 bytes fit");
}

#[test]
fn gateware_without_a_tx_mmcm_fails_the_create_like_libusdr() {
    let (outcomes, _) = compare(
        || SimBoard::new(Variant::Xsdr).with_gateware_build(TX_GATEWARE),
        &[Call::Rate(10_000_000), Call::CreateTx(TxChannels::A, 1024)],
    );
    assert_eq!(outcomes, [(-ERANGE, None), (-ERANGE, None)]);
}

/// The RX PHY reads before the TX calibration's: the rate's RX sweep, then the create's,
/// each on a clean link nine phases and the best-effort probe, ten checks of four
/// counters each.
const RX_SWEEP_READS: usize = 2 * 400;

/// A TX sweep phase whose first check reads 5 errors on each counter (badness 100), then
/// its I/Q-swap count 0.
const DIRTY_PHASE: [u32; 5] = [5, 5, 5, 5, 0];

/// One TX phase of the later sweeps: the RX recheck reads 1 error on each counter
/// (badness 4, too little to retry), then the phase as [`DIRTY_PHASE`].
const RECHECKED_DIRTY_PHASE: [u32; 9] = [1, 1, 1, 1, 5, 5, 5, 5, 0];

/// The phases a TX sweep takes.
const PHASES: usize = 252;

#[test]
fn a_tx_sweep_that_finds_no_phase_toggles_the_clock_and_rechecks_rx_like_libusdr() {
    // The first sweep finds nothing; then all is clean, so the second sweep's first
    // phase rechecks RX and passes.
    let script: Vec<u32> = std::iter::repeat_n(0, RX_SWEEP_READS)
        .chain((0..PHASES).flat_map(|_| DIRTY_PHASE))
        .collect();
    let (_, trace) = compare(
        || xsdr().with_rx_phy_reads(script.clone(), 0),
        &[
            Call::Rate(10_000_000),
            Call::CreateTx(TxChannels::A, 1024),
            Call::StartTx,
            Call::Send(1024, None),
            Call::StopTx,
        ],
    );
    // `lms7002m_limelight_toggle_ntx`'s first word: CDS_0x00AD with the TX processors'
    // and FIFOs' clocks uninverted.
    assert!(wrote_spi(&trace, 0x80ad_00cf), "the TX clocks toggled");
}

#[test]
fn a_misaligned_phase_rotates_the_lanes_until_libusdr_gives_up() {
    // The first phase's 35 LFSR checks and swap count are clean, but every alignment
    // count reads 1, which the first sweep does not accept: the lanes rotate until
    // attempt 20 gives up as good.
    let script: Vec<u32> = std::iter::repeat_n(0, RX_SWEEP_READS + 35 * 4 + 1)
        .chain(std::iter::repeat_n(1, 21))
        .collect();
    let (_, trace) = compare(
        || xsdr().with_rx_phy_reads(script.clone(), 0),
        &[
            Call::Rate(10_000_000),
            Call::CreateTx(TxChannels::Both, 1024),
            Call::StartTx,
            Call::Send(1024, None),
            Call::StopTx,
        ],
    );
    // Attempts 0 to 20 each rotate to the next selection (`msk[(g + 1) % 4]`).
    let rotations: usize = [0b0110, 0b0011, 0b1001, 0b1100]
        .into_iter()
        .map(|selection| writes(&trace, 57, 5 << 24 | selection))
        .sum();
    assert!(rotations >= 21, "{rotations} lane rotations");
}

#[test]
fn a_tx_link_no_sweep_finds_restores_its_first_phase_and_fails_like_libusdr() {
    // Eight sweeps find nothing (the later ones' RX rechecks pass with little badness);
    // the restored first phase's check reads 5 errors again: badness 100, rescaled by 10,
    // severe.
    let script: Vec<u32> = std::iter::repeat_n(0, RX_SWEEP_READS)
        .chain((0..PHASES).flat_map(|_| DIRTY_PHASE))
        .chain((0..7 * PHASES).flat_map(|_| RECHECKED_DIRTY_PHASE))
        .chain([5, 5, 5, 5, 0, 0])
        .collect();
    let (outcomes, trace) = compare(
        || xsdr().with_rx_phy_reads(script.clone(), 0),
        &[Call::Rate(10_000_000), Call::CreateTx(TxChannels::A, 1024)],
    );
    assert_eq!(outcomes[1].0, -ERANGE, "TX badness over 100");
    // The RX sweep's ten, eight sweeps of 252 phases, the restore.
    assert_eq!(writes(&trace, 57, MMCM_HELD), 1 + 10 + 8 * PHASES + 1);
}
