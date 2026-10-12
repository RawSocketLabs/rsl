//! rsl-usdr's RX gain and path controls against libusdr's, operation for operation, one
//! window per call, on fresh boards.
//!
//! Each call names libusdr's control path and raw value next to rsl-usdr's typed value, so
//! the pairing is the mapping under test (`rx/gain/pga` doubling its value, say). libusdr
//! clamps every gain rather than rejecting it, so out-of-range values must succeed alike.

#[path = "common/crossing.rs"]
mod crossing;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;
use std::ops::Range;

use crossing::{Crossing, with_band_crossing_fix};
use rsl_usdr::{Device, Error, RxGain, RxPath};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// libusdr's RX gain controls.
const GAIN: &CStr = c"/dm/sdr/0/rx/gain";
/// RXVGA1, the same handler as [`GAIN`].
const VGA: &CStr = c"/dm/sdr/0/rx/gain/vga";
/// RXVGA2 as one stage, in half-dB units: libusdr doubles the value.
const PGA: &CStr = c"/dm/sdr/0/rx/gain/pga";
/// RXVGA2's first stage.
const VGA2A: &CStr = c"/dm/sdr/0/rx/gain/vga2a";
/// RXVGA2's second stage.
const VGA2B: &CStr = c"/dm/sdr/0/rx/gain/vga2b";
/// The LNA's gain mode.
const LNA: &CStr = c"/dm/sdr/0/rx/gain/lna";
/// The automatic split.
const AUTO: &CStr = c"/dm/sdr/0/rx/gain/auto";
/// The RX path, a string.
const PATH: &CStr = c"/dm/sdr/0/rx/path";

/// One call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_frequency`.
    Frequency(u32),

    /// libusdr's numeric gain control `path` set to `value`; rsl-usdr's `set_rx_gain`.
    Gain(&'static CStr, u64, RxGain),

    /// libusdr's `rx/path` set to a string; rsl-usdr's `set_rx_path`.
    Path(&'static CStr, RxPath),
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate),
            Self::Frequency(hz) => oracle.set_rx_frequency(hz),
            // SAFETY: every RX gain handler reads its value as a number.
            Self::Gain(path, value, _) => unsafe { oracle.set_uint(path, value) },
            // SAFETY: `dev_m2_lm6_1_sdr_rx_path_set` reads a value above 4096 as a string
            // and keeps no reference to it.
            Self::Path(name, _) => unsafe { oracle.set_str(PATH, name) },
        }
        .is_ok()
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Frequency(hz) => device.set_rx_frequency(hz),
            Self::Gain(_, _, gain) => device.set_rx_gain(gain),
            Self::Path(_, path) => device.set_rx_path(path),
        }
    }
}

/// `db` as libusdr's 64-bit control value: a negative figure is its two's complement,
/// which the handlers truncate back to `int`.
fn raw(db: i32) -> u64 {
    u64::from_ne_bytes(i64::from(db).to_ne_bytes())
}

/// A gain call setting libusdr's `path` and rsl-usdr's `gain` to the same `db`.
fn gain(path: &'static CStr, db: i32, gain: fn(i32) -> RxGain) -> Call {
    Call::Gain(path, raw(db), gain(db))
}

/// Makes `calls` in turn on fresh revision-3 boards through libusdr and rsl-usdr and
/// compares each call's outcome and operations. Every call must succeed: a regression
/// that failed both sides alike would otherwise pass.
fn assert_calls_succeed(calls: &[Call]) {
    assert_calls_succeed_crossing(calls, Crossing::None);
}

/// As [`assert_calls_succeed`], with libusdr's band crossings replaced as `crossing` says;
/// [`Crossing::Fixed`] requires at least one.
fn assert_calls_succeed_crossing(calls: &[Call], crossing: Crossing) {
    let board = || SimBoard::new(BoardRevision::Rev3);
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

    let mut crossings = 0;
    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert!(expected_ok, "{call:?}: libusdr succeeds");
        assert!(result.is_ok(), "{call:?}: rsl-usdr {result:?}");
        let expected = &reference.trace()[expected];
        let expected: &[Op] = &match crossing {
            Crossing::None => expected.to_vec(),
            Crossing::Fixed => {
                let (fixed, crossed) = with_band_crossing_fix(expected);
                crossings += usize::from(crossed);
                fixed
            }
        };
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
    }
    if crossing == Crossing::Fixed {
        assert!(
            crossings > 0,
            "the scenario crosses the mixer band at least once"
        );
    }
}

/// RXVGA1 through both of libusdr's paths, across and beyond its 5 to 31 dB range; 31 dB
/// takes libusdr's 30.17 dB special case.
#[test]
fn rx_vga1_gain_matches_libusdr_across_its_range() {
    let mut calls = vec![gain(GAIN, 20, RxGain::Vga1)];
    for db in [i32::MIN, -1, 0, 4, 5, 6, 19, 25, 30, 31, 32, 1000, i32::MAX] {
        calls.push(gain(VGA, db, RxGain::Vga1));
    }
    assert_calls_succeed(&calls);
}

/// libusdr's `rx/gain/pga` doubles its value before clamping to 0 to 30 dB, so rsl-usdr's
/// [`RxGain::Vga2`] gets twice the figure; the combined setting also zeroes stage B.
#[test]
fn rx_vga2_combined_gain_matches_libusdr_with_its_doubling() {
    let mut calls = vec![gain(VGA2B, 12, RxGain::Vga2b)];
    for half_db in [-1, 0, 1, 2, 7, 14, 15, 16, 1000] {
        calls.push(Call::Gain(PGA, raw(half_db), RxGain::Vga2(2 * half_db)));
    }
    assert_calls_succeed(&calls);
}

/// Each RXVGA2 stage keeps the other's last step, in 3 dB steps rounded down and clamped
/// to 0 to 30 dB.
#[test]
fn rx_vga2_stages_are_independent_like_libusdr() {
    assert_calls_succeed(&[
        gain(VGA2A, 9, RxGain::Vga2a),
        gain(VGA2B, 14, RxGain::Vga2b),
        gain(VGA2A, 31, RxGain::Vga2a),
        gain(VGA2B, -3, RxGain::Vga2b),
        gain(VGA2B, 30, RxGain::Vga2b),
        gain(VGA2A, 2, RxGain::Vga2a),
    ]);
}

/// The LNA's three modes, either side of its 3 and 6 dB thresholds.
#[test]
fn rx_lna_gain_matches_libusdr_at_its_thresholds() {
    let calls: Vec<Call> = [i32::MIN, -1, 0, 2, 3, 5, 6, 100]
        .into_iter()
        .map(|db| gain(LNA, db, RxGain::Lna))
        .collect();
    assert_calls_succeed(&calls);
}

/// The automatic split across its 0 to 85 dB range and beyond, untuned (libusdr's table
/// held at 230 MHz) and at frequencies on, between and at the ends of its LNA table.
#[test]
fn rx_auto_gain_matches_libusdr_across_frequencies() {
    let figures = [i32::MIN, -1, 0, 10, 15, 21, 26, 46, 47, 60, 85, 1000];
    let mut calls = vec![Call::Rate(20_000_000)];
    calls.extend(figures.map(|db| gain(AUTO, db, RxGain::Auto)));
    for hz in [433_000_000, 1_050_000_000, 2_400_000_000, 3_800_000_000] {
        calls.push(Call::Frequency(hz));
        calls.extend(figures.map(|db| gain(AUTO, db, RxGain::Auto)));
    }
    assert_calls_succeed(&calls);
}

/// Fixed paths, named as libusdr spells them in any case, hold across retunes and rate
/// changes; entering and leaving the low band gate the mixer LO (with the band-crossing
/// fix); `auto` and an unknown name hand the band back to the frequency.
#[test]
fn manual_rx_paths_match_libusdr() {
    assert_calls_succeed_crossing(
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Path(c"rxh", RxPath::High),
            Call::Frequency(433_000_000),
            Call::Path(c"RXW", RxPath::Wide),
            Call::Frequency(3_000_000_000),
            Call::Path(c"rxl", RxPath::Low),
            Call::Frequency(433_000_000),
            Call::Rate(10_000_000),
            Call::Path(c"auto", RxPath::Auto),
            Call::Path(c"rxh", RxPath::High),
            Call::Path(c"no-such-path", RxPath::Auto),
        ],
        Crossing::Fixed,
    );
}

/// libusdr's band names reach the same paths as its `rx*` names.
#[test]
fn libusdr_band_names_select_the_same_paths() {
    assert_calls_succeed(&[
        Call::Rate(20_000_000),
        Call::Frequency(1_000_000_000),
        Call::Path(c"LNAH", RxPath::High),
        Call::Path(c"w", RxPath::Wide),
        Call::Path(c"H", RxPath::High),
        Call::Path(c"lnaw", RxPath::Wide),
    ]);
}

/// The loopback paths switch the RF loopback on once, keep it across a change of loopback
/// input (still on the first input's LNA path, as libusdr leaves it), and off again when
/// a plain or automatic path follows.
#[test]
fn rf_loopback_paths_match_libusdr() {
    assert_calls_succeed_crossing(
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Path(c"rxw_lb", RxPath::WideLoopback),
            Call::Path(c"RXH_LB", RxPath::HighLoopback),
            Call::Path(c"rxw", RxPath::Wide),
            Call::Path(c"rxl_lb", RxPath::LowLoopback),
            Call::Path(c"auto", RxPath::Auto),
        ],
        Crossing::Fixed,
    );
}

/// Before any frequency is set, `auto` selects the band for 0 Hz: the low band, mixer on.
#[test]
fn auto_rx_path_before_tuning_selects_the_low_band_like_libusdr() {
    assert_calls_succeed_crossing(
        &[Call::Rate(20_000_000), Call::Path(c"auto", RxPath::Auto)],
        Crossing::Fixed,
    );
}

/// libusdr rejects a path given as a number (4096 or less) with `-EINVAL` before any
/// operation. rsl-usdr's [`RxPath`] cannot express one, so there is no call to compare.
#[test]
fn a_numeric_rx_path_is_rejected_by_libusdr_before_any_operation() {
    /// `EINVAL`.
    const EINVAL: i32 = 22;
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("opens");
    for value in [0, 1, 4096] {
        let start = oracle.trace_len();
        // SAFETY: the handler rejects values of 4096 or less without reading them.
        let result = unsafe { oracle.set_uint(PATH, value) };
        assert_eq!(result, Err(-EINVAL), "libusdr rejects {value}");
        assert_eq!(oracle.trace_len(), start, "{value}: no operation");
    }
    drop(oracle.close());
}
