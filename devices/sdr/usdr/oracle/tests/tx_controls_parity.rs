//! usdr's TX controls (frequency, path, bandwidth, gains, corrections, constant
//! generator) against libusdr's, operation for operation, one window per call, on fresh
//! boards.
//!
//! Each call names libusdr's control path and raw value next to usdr's typed value, so
//! the pairing is the mapping under test. libusdr clamps every gain rather than rejecting it,
//! so out-of-range figures must succeed alike; tuning failures must fail alike, after the
//! same operations. libusdr keeps the low 12 bits of a TX DC offset where usdr's
//! [`TxDcOffset`] rejects one that does not fit; that divergence has its own scenario.
//!
//! Every scenario that tunes the transmitter sets a sample rate first: before one, libusdr
//! divides by zero updating the TX filter, which would kill the test process.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;
use std::ops::Range;

use num_complex::Complex;
use sim_bus::SimBus;
use usdr::{Device, Error, TxDcOffset, TxGain, TxIqCorrection, TxPath};
use usdr_oracle::Oracle;
use usdr_sim::{BoardRevision, Op, PllLock, SimBoard};

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
/// The TX DC offsets, I in bits 31:16 and Q in bits 15:0.
const DCCORR: &CStr = c"/dm/sdr/0/tx/dccorr";
/// The IQ correction: I gain in bits 15:0, Q gain in 31:16, phase in 63:48.
const PHGAINCORR: &CStr = c"/dm/sdr/0/tx/phgaincorr";
/// The constant generator: I in bits 31:16, Q in 15:0, or all ones for off.
const GENERATOR: &CStr = c"/dm/sdr/0/tfe/generator/const";

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

    /// libusdr's `tx/path` set to a string; usdr's `set_tx_path`.
    Path(&'static CStr, TxPath),

    /// libusdr's numeric control `path` set to `value`; usdr's matching call.
    Set(&'static CStr, u64, Typed),
}

/// usdr's side of a [`Call::Set`].
#[derive(Clone, Copy, Debug)]
enum Typed {
    /// `set_tx_gain`.
    Gain(TxGain),

    /// `set_tx_dc_offset`.
    DcOffset(TxDcOffset, TxDcOffset),

    /// `set_tx_iq_correction`.
    Iq(TxIqCorrection),

    /// `set_tx_constant`.
    Constant(Option<Complex<i16>>),
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
            // SAFETY: every TX gain, correction and generator handler reads its value as a
            // number.
            Self::Set(path, value, _) => unsafe { oracle.set_uint(path, value) },
        }
        .is_ok()
    }

    /// Makes the call through usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::RxFrequency(hz) => device.set_rx_frequency(hz),
            Self::Frequency(hz) => device.set_tx_frequency(hz),
            Self::Bandwidth(hz) => device.set_tx_bandwidth(hz),
            Self::Path(_, path) => device.set_tx_path(path),
            Self::Set(_, _, Typed::Gain(gain)) => device.set_tx_gain(gain),
            Self::Set(_, _, Typed::DcOffset(i, q)) => device.set_tx_dc_offset(i, q),
            Self::Set(_, _, Typed::Iq(correction)) => device.set_tx_iq_correction(correction),
            Self::Set(_, _, Typed::Constant(constant)) => device.set_tx_constant(constant),
        }
    }
}

/// `value`'s 16 two's-complement bits, as libusdr's packed controls carry them.
fn bits(value: i16) -> u64 {
    u64::from(u16::from_ne_bytes(value.to_ne_bytes()))
}

/// A gain call setting libusdr's `path` and usdr's `gain` to the same `db`; a negative
/// figure travels as its 64-bit two's complement, which the handler truncates to `int`.
fn gain(path: &'static CStr, db: i32, gain: fn(i32) -> TxGain) -> Call {
    let value = u64::from_ne_bytes(i64::from(db).to_ne_bytes());
    Call::Set(path, value, Typed::Gain(gain(db)))
}

/// The offset `value`, which must fit.
fn offset(value: i16) -> TxDcOffset {
    TxDcOffset::new(value).expect("a 12-bit offset")
}

/// A DC offset call with `i` and `q`, packed as `dev_m2_lm6_1_sdr_tx_dccorr_set` unpacks it.
fn dc(i: i16, q: i16) -> Call {
    let value = bits(i) << 16 | bits(q);
    Call::Set(DCCORR, value, Typed::DcOffset(offset(i), offset(q)))
}

/// An IQ correction call, packed as `dev_m2_lm6_1_sdr_tx_phgaincorr_set` unpacks it.
fn iq(i_gain: u16, q_gain: u16, phase: i16) -> Call {
    let value = bits(phase) << 48 | u64::from(q_gain) << 16 | u64::from(i_gain);
    let correction = TxIqCorrection {
        i_gain,
        q_gain,
        phase,
    };
    Call::Set(PHGAINCORR, value, Typed::Iq(correction))
}

/// A constant generator call: `Some((i, q))` as libusdr packs it, `None` as all ones.
fn constant(pair: Option<(i16, i16)>) -> Call {
    let value = pair.map_or(u64::MAX, |(i, q)| bits(i) << 16 | bits(q));
    let typed = pair.map(|(i, q)| Complex::new(i, q));
    Call::Set(GENERATOR, value, Typed::Constant(typed))
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// Makes `calls` in turn on fresh boards from `board` through libusdr and usdr,
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
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up the board");
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
            "{call:?}: libusdr ok={expected_ok}, usdr {result:?}"
        );
        let expected: &[Op] = &reference.trace()[expected];
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{call:?}: first difference at op {i}: libusdr {:x?}, usdr {:x?}",
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

/// TX DC offsets over sign and both ends of the 12-bit range; libusdr ignores the channel
/// number above bit 31.
#[test]
fn tx_dc_offsets_match_libusdr() {
    let mut calls = vec![
        dc(0, 0),
        dc(2047, -2048),
        dc(-1, 1),
        dc(-2048, 2047),
        dc(564, -564),
    ];
    let Call::Set(path, value, typed) = dc(0x123, -0x123) else {
        unreachable!("dc makes a set call");
    };
    calls.push(Call::Set(path, value | 1 << 32, typed));
    assert_calls_succeed(rev3, &calls);
}

/// Intentional divergence: libusdr keeps the low 12 bits of each 16-bit offset, so an
/// oversized one silently writes another. usdr's [`TxDcOffset`] rejects it before any
/// operation; what libusdr writes for one is the masked offset's write.
#[test]
fn oversized_tx_dc_offsets_are_masked_by_libusdr_and_rejected_by_rsl_usdr() {
    let oversized = [2048, -2049, i16::MAX, i16::MIN, 0x1234];
    for value in oversized {
        assert_eq!(TxDcOffset::new(value), None, "{value} is rejected");
    }
    // The 12 bits libusdr keeps, read back as a 12-bit two's-complement offset.
    let masked = |value: i16| {
        let low = i32::from(u16::from_ne_bytes(value.to_ne_bytes()) & 0xfff);
        let signed = if low > 2047 { low - 4096 } else { low };
        offset(i16::try_from(signed).expect("12 bits fit"))
    };
    let calls: Vec<Call> = oversized
        .windows(2)
        .map(|pair| {
            let value = bits(pair[0]) << 16 | bits(pair[1]);
            Call::Set(
                DCCORR,
                value,
                Typed::DcOffset(masked(pair[0]), masked(pair[1])),
            )
        })
        .collect();
    assert_calls_succeed(rev3, &calls);
}

/// The TX IQ correction's float arithmetic at the TX chain's 3 dB headroom scale, term for
/// term: no correction, either gain side of 2047, both extremes of the phase, and garbage
/// in libusdr's unused bits 47:32.
#[test]
fn tx_iq_correction_matches_libusdr() {
    let mut calls = vec![
        iq(2047, 2047, 0),
        iq(0, 9999, 0),
        iq(1023, 2047, 1000),
        iq(2046, 2048, 1),
        iq(2047, 0, -1000),
        iq(2047, u16::MAX, i16::MAX),
        iq(u16::MAX, 4095, i16::MIN),
        iq(3000, 1500, 2812),
    ];
    let Call::Set(path, value, typed) = iq(1500, 2500, -2812) else {
        unreachable!("iq makes a set call");
    };
    calls.push(Call::Set(path, value | 0xabcd << 32, typed));
    assert_calls_succeed(rev3, &calls);
}

/// The constant generator on with zero, mixed-sign and extreme pairs (all-ones bits that
/// are not libusdr's off value included), off, off again, and on once more.
#[test]
fn the_tx_constant_generator_matches_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            constant(Some((0, 0))),
            constant(Some((1234, -5678))),
            constant(Some((i16::MAX, i16::MIN))),
            constant(Some((-1, -1))),
            constant(None),
            constant(None),
            constant(Some((1, 0))),
        ],
    );
}
