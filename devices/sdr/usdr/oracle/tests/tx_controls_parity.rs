//! rsl-usdr's TX controls (frequency, path, bandwidth, gains) against libusdr's, operation
//! for operation, one window per call, on fresh boards.
//!
//! Each call names libusdr's control path and raw value next to rsl-usdr's typed value, so
//! the pairing is the mapping under test. libusdr clamps every gain rather than rejecting it,
//! so out-of-range figures must succeed alike; tuning failures must fail alike, after the
//! same operations.
//!
//! Every scenario that tunes the transmitter sets a sample rate first: before one, libusdr
//! divides by zero updating the TX filter, which would kill the test process.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;
use std::ops::Range;

use rsl_usdr::{Device, Error, TxGain, TxPath};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, PllLock, SimBoard};
use sim_bus::SimBus;

/// TXVGA2, as libusdr's plain TX gain.
const GAIN: &CStr = c"/dm/sdr/0/tx/gain";
/// TXVGA1.
const VGA1: &CStr = c"/dm/sdr/0/tx/gain/vga1";
/// TXVGA2.
const VGA2: &CStr = c"/dm/sdr/0/tx/gain/vga2";
/// libusdr's automatic TX gain: TXVGA2 again.
const AUTO: &CStr = c"/dm/sdr/0/tx/gain/auto";
/// The TX path, a string.
const PATH: &CStr = c"/dm/sdr/0/tx/path";

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_frequency`.
    RxFrequency(u32),

    /// `set_tx_frequency`.
    Frequency(u32),

    /// `set_tx_bandwidth`.
    Bandwidth(u32),

    /// libusdr's `tx/path` set to a string; rsl-usdr's `set_tx_path`.
    Path(&'static CStr, TxPath),

    /// libusdr's numeric control `path` set to `value`; rsl-usdr's matching call.
    Set(&'static CStr, u64, Typed),
}

/// rsl-usdr's side of a [`Call::Set`].
#[derive(Clone, Copy, Debug)]
enum Typed {
    /// `set_tx_gain`.
    Gain(TxGain),
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate),
            Self::RxFrequency(hz) => oracle.set_rx_frequency(hz),
            Self::Frequency(hz) => oracle.set_tx_frequency(hz),
            Self::Bandwidth(hz) => oracle.set_tx_bandwidth(hz),
            // SAFETY: `dev_m2_lm6_1_sdr_tx_path_set` reads a value above 4096 as a string
            // and keeps no reference to it; a string's address is always above 4096 here.
            Self::Path(name, _) => unsafe { oracle.set_str(PATH, name) },
            // SAFETY: every TX gain handler reads its value as a number.
            Self::Set(path, value, _) => unsafe { oracle.set_uint(path, value) },
        }
        .is_ok()
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::RxFrequency(hz) => device.set_rx_frequency(hz),
            Self::Frequency(hz) => device.set_tx_frequency(hz),
            Self::Bandwidth(hz) => device.set_tx_bandwidth(hz),
            Self::Path(_, path) => device.set_tx_path(path),
            Self::Set(_, _, Typed::Gain(gain)) => device.set_tx_gain(gain),
        }
    }
}

/// A gain call setting libusdr's `path` and rsl-usdr's `gain` to the same `db`; a negative
/// figure travels as its 64-bit two's complement, which the handler truncates to `int`.
fn gain(path: &'static CStr, db: i32, gain: fn(i32) -> TxGain) -> Call {
    let value = u64::from_ne_bytes(i64::from(db).to_ne_bytes());
    Call::Set(path, value, Typed::Gain(gain(db)))
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// Makes `calls` in turn on fresh boards from `board` through libusdr and rsl-usdr,
/// compares each call's outcome and operations, and returns whether each succeeded.
fn assert_calls_match(board: impl Fn() -> SimBoard, calls: &[Call]) -> Vec<bool> {
    let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
    let mut expected: Vec<(bool, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = oracle.trace_len();
        let ok = call.on_libusdr(&mut oracle);
        expected.push((ok, start..oracle.trace_len()));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual: Vec<(Result<(), Error>, Range<usize>)> = Vec::new();
    for &call in calls {
        let start = sim.board().trace().len();
        let result = call.on_rsl(&mut device);
        actual.push((result, start..sim.board().trace().len()));
    }
    let board = sim.board();

    let mut outcomes = Vec::new();
    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert_eq!(
            result.is_ok(),
            expected_ok,
            "{call:?}: libusdr ok={expected_ok}, rsl-usdr {result:?}"
        );
        let expected: &[Op] = &reference.trace()[expected];
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{call:?}: first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
                expected[i], actual[i]
            );
        }
        assert_eq!(
            expected.len(),
            actual.len(),
            "{call:?}: same prefix, different lengths"
        );
        outcomes.push(expected_ok);
    }
    outcomes
}

/// As [`assert_calls_match`], and every call must succeed: a regression that failed both
/// sides alike would otherwise pass.
fn assert_calls_succeed(board: impl Fn() -> SimBoard, calls: &[Call]) {
    let outcomes = assert_calls_match(board, calls);
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {calls:?} -> {outcomes:?}"
    );
}

/// The first tune powers the transmitter; each PA band, across VCO ranges and at the
/// 2.8 GHz edge, selects its amplifier and switch, and the filter follows the rate.
#[test]
fn tx_tuning_across_both_bands_matches_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Frequency(433_000_000),
            Call::Frequency(2_799_999_999),
            Call::Frequency(2_800_000_000),
            Call::Frequency(3_800_000_000),
            Call::Frequency(250_000_000),
        ],
    );
}

/// Revision 1 tunes the transmitter alike, its reference on another Si5332 output.
#[test]
fn rev1_tx_tuning_matches_libusdr() {
    assert_calls_succeed(
        || SimBoard::new(BoardRevision::Rev1),
        &[Call::Rate(1_000_000), Call::Frequency(915_000_000)],
    );
}

/// Below the PLL's 170 MHz floor both refuse, after powering the transmitter and selecting
/// its band; 170 MHz itself tunes.
#[test]
fn a_tx_frequency_below_the_pll_floor_fails_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(169_999_999),
            Call::Frequency(170_000_000),
        ],
    );
    assert_eq!(outcomes, [true, false, true]);
}

/// A board whose TX PLL locks only from 245 MHz up.
fn rev3_tx_unlocked_below_245m() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3).with_tx_pll_lock(PllLock {
        unlocked_below_hz: 245_000_000,
        ..PllLock::default()
    })
}

/// 240 MHz cannot lock. As the first TX tune with no RX frequency, libusdr tunes the RX
/// PLL there before retrying, then steps up to 245 MHz and covers the 5 MHz with the TX
/// NCOs; the next low tune reuses 245 MHz, and a rate change (1.5 MS/s, a 48 MS/s DAC
/// against the first's 32) re-points the TX NCOs at the new rate.
#[test]
fn the_tx_low_lo_fallback_matches_libusdr() {
    assert_calls_succeed(
        rev3_tx_unlocked_below_245m,
        &[
            Call::Rate(1_000_000),
            Call::Frequency(240_000_000),
            Call::Frequency(241_000_000),
            Call::Rate(1_500_000),
            Call::Frequency(1_000_000_000),
        ],
    );
}

/// With an RX frequency set, the first failed TX tune does not touch the RX PLL.
#[test]
fn a_tuned_receiver_skips_the_rx_pll_retry_like_libusdr() {
    assert_calls_succeed(
        rev3_tx_unlocked_below_245m,
        &[
            Call::Rate(1_000_000),
            Call::RxFrequency(1_000_000_000),
            Call::Frequency(240_000_000),
        ],
    );
}

/// At 20 MS/s (a 40 MS/s DAC) the 5 MHz NCO offset does not fit, so tuning fails on both
/// sides, after the same operations.
#[test]
fn a_tx_fallback_without_nco_room_fails_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3_tx_unlocked_below_245m,
        &[Call::Rate(20_000_000), Call::Frequency(240_000_000)],
    );
    assert_eq!(outcomes, [true, false]);
}

/// Above 250 MHz there is no fallback: a TX PLL that cannot lock fails the call, the first
/// time with the RX PLL retry and the second without.
#[test]
fn an_unlockable_tx_lo_above_250m_fails_like_libusdr() {
    let outcomes = assert_calls_match(
        || {
            SimBoard::new(BoardRevision::Rev3).with_tx_pll_lock(PllLock {
                unlocked_below_hz: 2_000_000_000,
                ..PllLock::default()
            })
        },
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Frequency(1_000_000_000),
        ],
    );
    assert_eq!(outcomes, [true, false, false]);
}

/// `auto` before any frequency selects PA1; a fixed path holds across retunes in and out
/// of its band, `auto` and unknown names hand it back to the frequency at once, and
/// libusdr's names match in any case and either spelling.
#[test]
fn tx_paths_match_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Path(c"auto", TxPath::Auto),
            Call::Frequency(1_000_000_000),
            Call::Path(c"txh", TxPath::High),
            Call::Frequency(1_500_000_000),
            Call::Path(c"auto", TxPath::Auto),
            Call::Frequency(3_000_000_000),
            Call::Path(c"TXW", TxPath::Wide),
            Call::Frequency(3_500_000_000),
            Call::Path(c"b2", TxPath::High),
            Call::Path(c"w", TxPath::Wide),
            Call::Path(c"H", TxPath::High),
            Call::Path(c"B1", TxPath::Wide),
            Call::Path(c"no-such-path", TxPath::Auto),
        ],
    );
}

/// A fixed bandwidth survives TX retunes and rate changes; 0 writes the narrowest filter
/// and hands the filter back to the rate at the next tune; above 47 MHz it is bypassed.
#[test]
fn tx_bandwidth_matches_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Bandwidth(5_000_000),
            Call::Frequency(1_000_000_000),
            Call::Rate(10_000_000),
            Call::Frequency(1_100_000_000),
            Call::Bandwidth(0),
            Call::Frequency(1_000_000_000),
            Call::Bandwidth(50_000_000),
            Call::Bandwidth(1),
        ],
    );
}

/// TXVGA1 across and beyond its -35 to -4 dB range, untuned.
#[test]
fn tx_vga1_gain_matches_libusdr_across_its_range() {
    let calls: Vec<Call> = [i32::MIN, -36, -35, -34, -20, -5, -4, -3, 0, i32::MAX]
        .into_iter()
        .map(|db| gain(VGA1, db, TxGain::Vga1))
        .collect();
    assert_calls_succeed(rev3, &calls);
}

/// TXVGA2 across and beyond its 0 to 25 dB range, through each of libusdr's three paths to
/// it, untuned.
#[test]
fn tx_vga2_gain_matches_libusdr_through_all_three_controls() {
    let mut calls = Vec::new();
    for path in [GAIN, VGA2, AUTO] {
        for db in [i32::MIN, -1, 0, 1, 12, 24, 25, 26, i32::MAX] {
            calls.push(gain(path, db, TxGain::Vga2));
        }
    }
    assert_calls_succeed(rev3, &calls);
}
