//! lsdr's sample rate, receive frequency (NCO) and temperature against libusdr's,
//! operation for operation and outcome for outcome, on fresh simulated boards: every step
//! of a session between its open and its close, the close included.
//!
//! Rates reach every decimation and LVDS clock tap, the Si549's range edges and, on a
//! dual-ADC board opened with `lmk=1`, the LMK04832's layout and lock check; frequencies
//! reach the NCO's folding (offsets past the ADC rate, past 32 bits and below zero) and,
//! with fe1005000 attached, its tuning and refusals.
//!
//! Whitelisted divergence, none: no scenario here injects a bus fault, where lsdr reports
//! failures libusdr ignores (`PORTING.md`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use lsdr::{ClockPlanFailure, Device, Error, OpenOptions};
use lsdr_oracle::Oracle;
use lsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;

/// One step of a session after open.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// `/dm/power/en`.
    Power(bool),
    /// `/dm/rate/master`.
    Rate(u32),
    /// `/dm/sdr/0/rx/frequency`.
    Frequency(u64),
    /// `/dm/sensor/temp`.
    Temperature,
}

/// A step's outcome as both drivers report it: a temperature word in 1/256 °C, or the
/// negative errno libusdr returns (lsdr's error mapped to it).
type Outcome = Result<Option<i64>, i32>;

/// A board and how to open it.
#[derive(Clone, Copy, Debug)]
struct Fit {
    /// The board.
    variant: Variant,
    /// fe1005000 on its connector.
    front_end: bool,
    /// A TMP108 word, if not the sim's default.
    temperature: Option<i16>,
    /// libusdr's connection string.
    connection: &'static CStr,
    /// lsdr's equivalent.
    options: OpenOptions,
}

impl Fit {
    /// `variant` with nothing attached, opened with defaults.
    fn new(variant: Variant) -> Self {
        Self {
            variant,
            front_end: false,
            temperature: None,
            connection: c"",
            options: OpenOptions::new(),
        }
    }

    /// The dual-ADC board opened with `lmk=1`, so its LMK04832 is found.
    fn clocked() -> Self {
        Self {
            connection: c"lmk=1",
            options: OpenOptions::new().probe_clock_generator(true),
            ..Self::new(Variant::DualAdcClockGen)
        }
    }

    /// A fresh board.
    fn board(self) -> SimBoard {
        let mut board = SimBoard::new(self.variant);
        if self.front_end {
            board = board.with_fe1005000();
        }
        if let Some(word) = self.temperature {
            board = board.with_temperature(word);
        }
        board
    }
}

/// Each step's operations and outcome, then the close's operations.
#[derive(Debug)]
struct Session {
    /// Each step's operations and outcome.
    steps: Vec<(Vec<Op>, Outcome)>,
    /// The close's operations.
    close: Vec<Op>,
}

/// Splits `trace` at `ends`: the parts between consecutive ends, then the rest.
fn split(trace: &[Op], ends: &[usize]) -> (Vec<Vec<Op>>, Vec<Op>) {
    let parts = ends
        .windows(2)
        .map(|w| trace[w[0]..w[1]].to_vec())
        .collect();
    let rest = trace[*ends.last().expect("an end")..].to_vec();
    (parts, rest)
}

/// libusdr's session.
fn libusdr(fit: Fit, steps: &[Step]) -> Session {
    let mut oracle = Oracle::open(fit.board(), fit.connection).expect("libusdr opens the board");
    let mut ends = vec![oracle.trace_len()];
    let mut outcomes = Vec::new();
    for step in steps {
        outcomes.push(match *step {
            Step::Power(on) => oracle.set_power(on).map(|()| None),
            Step::Rate(rate) => oracle.set_sample_rate(rate).map(|()| None),
            Step::Frequency(hz) => oracle.set_rx_frequency(hz).map(|()| None),
            Step::Temperature => oracle.temperature().map(Some),
        });
        ends.push(oracle.trace_len());
    }
    let board = oracle.close();
    let (parts, close) = split(board.trace(), &ends);
    Session {
        steps: parts.into_iter().zip(outcomes).collect(),
        close,
    }
}

/// The errno libusdr returns where lsdr returns `error`.
fn errno(error: &Error) -> i32 {
    match error {
        Error::UnsupportedSampleRate { .. }
        | Error::ClockPlan {
            source: ClockPlanFailure::NoVco,
            ..
        } => -ERANGE,
        Error::ClockPlan { .. } | Error::NoSampleRate | Error::FrontEnd(_) => -EINVAL,
        other => panic!("no libusdr failure in these scenarios matches {other:?}"),
    }
}

/// lsdr's session.
fn lsdr(fit: Fit, steps: &[Step]) -> Session {
    let sim = SimBus::new(fit.board());
    let mut device = Device::open(sim.clone(), fit.options).expect("lsdr opens the board");
    let mut ends = vec![sim.board().trace().len()];
    let mut outcomes = Vec::new();
    for step in steps {
        let outcome = match *step {
            Step::Power(on) => device.set_power(on).map(|()| None),
            Step::Rate(rate) => device.set_sample_rate(rate).map(|()| None),
            Step::Frequency(hz) => device.set_rx_frequency(hz).map(|()| None),
            Step::Temperature => device.temperature().map(|celsius| Some(word(celsius))),
        };
        outcomes.push(outcome.map_err(|error| errno(&error)));
        ends.push(sim.board().trace().len());
    }
    device.close().expect("lsdr closes the board");
    let (parts, close) = split(sim.board().trace(), &ends);
    Session {
        steps: parts.into_iter().zip(outcomes).collect(),
        close,
    }
}

/// A temperature in °C as libusdr's word in 1/256 °C.
fn word(celsius: f32) -> i64 {
    let scaled = celsius * 256.0;
    assert_eq!(scaled.fract(), 0.0, "a whole number of 1/256 °C");
    #[expect(clippy::cast_possible_truncation, reason = "a 16-bit sensor word")]
    i64::from(scaled as i32)
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

/// Runs `steps` on `fit` in both drivers and compares each step and the close; returns
/// libusdr's outcomes.
fn assert_same_steps(fit: Fit, steps: &[Step]) -> Vec<Outcome> {
    let expected = libusdr(fit, steps);
    let actual = lsdr(fit, steps);
    for (at, (step, ((want_ops, want), (ops, got)))) in steps
        .iter()
        .zip(expected.steps.iter().zip(&actual.steps))
        .enumerate()
    {
        let scenario = format!("{fit:?}, step {at} {step:?}");
        assert_same_ops(want_ops, ops, &scenario);
        assert_eq!(want, got, "{scenario}: outcomes differ");
    }
    assert_same_ops(&expected.close, &actual.close, &format!("{fit:?}, close"));
    expected
        .steps
        .into_iter()
        .map(|(_, outcome)| outcome)
        .collect()
}

#[test]
fn every_decimation_and_clock_tap_matches_libusdr_on_every_fit() {
    // x128, x64, x32, x16, x8, x6, x5, x4, x3, x2; then the taps from 110 MS/s up.
    let rates = [
        1_000_000,
        2_100_000,
        5_000_000,
        10_000_000,
        30_000_000,
        40_000_000,
        50_000_000,
        60_000_000,
        // x4 is exactly 260 MS/s, not under it: x3.
        65_000_000,
        80_000_000,
        100_000_000,
        110_000_000,
        160_000_000,
        275_000_000,
        290_000_000,
        300_000_000,
        320_000_000,
    ];
    let steps: Vec<Step> = rates.into_iter().map(Step::Rate).collect();
    for fit in [
        Fit::new(Variant::SingleAdc),
        Fit::new(Variant::DualAdcClockGen),
        Fit::clocked(),
    ] {
        let outcomes = assert_same_steps(fit, &steps);
        assert!(outcomes.iter().all(Result::is_ok), "{fit:?}: {outcomes:?}");
    }
}

#[test]
fn the_oscillators_range_is_refused_before_any_access_like_libusdr() {
    // 400 MS/s is an 800 MHz ADC clock, the Si549's maximum; 1563 S/s x128 is its
    // minimum's neighbour above, 1562 below. A rate whose ADC rate wraps 32 bits lands
    // back in the Si549's range in both, and then the LMK04832 has no dividers for it (its
    // VCO range check wraps too): refused after the gearbox is loaded.
    let steps = [
        Step::Rate(400_000_000),
        Step::Rate(400_000_001),
        Step::Rate(1_563),
        Step::Rate(1_562),
        Step::Rate(0),
        Step::Rate(2_247_483_648),
    ];
    let outcomes = assert_same_steps(Fit::clocked(), &steps);
    assert_eq!(
        outcomes,
        [
            Ok(None),
            Err(-ERANGE),
            Ok(None),
            Err(-ERANGE),
            Err(-ERANGE),
            Err(-EINVAL)
        ]
    );
}

#[test]
fn the_nco_matches_libusdr_before_and_after_a_rate() {
    let steps = [
        Step::Frequency(1_000_000),
        Step::Rate(50_000_000),
        Step::Frequency(0),
        Step::Frequency(10_000_000),
        Step::Frequency(62_500_000),
        Step::Frequency(250_000_005),
        Step::Frequency(500_000_000),
        Step::Frequency(5_000_000_000),
        Step::Frequency(u64::MAX - 999),
        // A refused rate keeps its ADC rate for the NCO.
        Step::Rate(400_000_001),
        Step::Frequency(10_000_000),
    ];
    for fit in [Fit::new(Variant::SingleAdc), Fit::clocked()] {
        let outcomes = assert_same_steps(fit, &steps);
        assert_eq!(outcomes[0], Err(-EINVAL), "no rate yet");
        assert!(outcomes[2..9].iter().all(Result::is_ok), "{outcomes:?}");
    }
}

#[test]
fn with_fe1005000_the_frequency_tunes_the_front_end_then_the_nco_like_libusdr() {
    let fit = Fit {
        front_end: true,
        ..Fit::new(Variant::SingleAdc)
    };
    let steps = [
        Step::Rate(50_000_000),
        Step::Frequency(900_000_000),
        Step::Frequency(1_500_123_456),
        Step::Frequency(19_999_999),
        Step::Frequency(10_000_000),
        Step::Frequency(6_000_000_000),
        // Above 2^32 Hz libusdr's unsigned front-end frequency wraps: to 2.705 GHz, which
        // tunes, and to 5.03 MHz, which is refused.
        Step::Frequency(7_000_000_000),
        Step::Frequency(4_300_000_000),
    ];
    let outcomes = assert_same_steps(fit, &steps);
    assert_eq!(
        outcomes,
        [
            Ok(None),
            Ok(None),
            Ok(None),
            Err(-EINVAL),
            Err(-EINVAL),
            Ok(None),
            Ok(None),
            Err(-EINVAL)
        ]
    );
}

#[test]
fn the_temperature_and_a_rate_after_power_match_libusdr() {
    for word in [None, Some(-0x0180), Some(0x7ff0)] {
        let fit = Fit {
            temperature: word,
            ..Fit::new(Variant::SingleAdc)
        };
        let steps = [
            Step::Temperature,
            Step::Power(true),
            Step::Rate(50_000_000),
            Step::Power(false),
        ];
        let outcomes = assert_same_steps(fit, &steps);
        assert_eq!(
            outcomes[0],
            Ok(Some(i64::from(word.unwrap_or(25 * 256)))),
            "libusdr returns the sensor's word"
        );
    }
}
