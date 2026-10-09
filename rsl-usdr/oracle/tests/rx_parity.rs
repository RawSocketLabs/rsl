//! rsl-usdr's RX controls (sample rate, bandwidth, frequency) against libusdr's, operation
//! for operation, over sequences of calls.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::{Device, Error};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, I2cAddress, Op, PllLock, SimBoard};
use sim_bus::SimBus;

/// One RX call, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `set_rx_sample_rate`.
    Rate(u32),

    /// `set_rx_bandwidth`.
    Bandwidth(u32),

    /// `set_rx_frequency`.
    Frequency(u32),
}

impl Call {
    /// Makes the call through libusdr, returning whether it succeeded.
    fn on_libusdr(self, oracle: &mut Oracle) -> bool {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate),
            Self::Bandwidth(hz) => oracle.set_rx_bandwidth(hz),
            Self::Frequency(hz) => oracle.set_rx_frequency(hz),
        }
        .is_ok()
    }

    /// Makes the call through rsl-usdr.
    fn on_rsl(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::Bandwidth(hz) => device.set_rx_bandwidth(hz),
            Self::Frequency(hz) => device.set_rx_frequency(hz),
        }
    }
}

/// The Si5332.
const CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };

/// Whether a scenario expects rsl-usdr's band-crossing fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crossing {
    /// The scenario never enters or leaves the mixer band.
    None,

    /// The scenario crosses into or out of the mixer band, so libusdr's
    /// `si5332_set_port3_en` block is expected to be replaced by rsl-usdr's one write.
    Fixed,
}

/// libusdr's window with its `si5332_set_port3_en` block replaced by the single `OUT3_OE`
/// write rsl-usdr makes instead, and whether the window had one.
///
/// Intentional divergence, scoped to the scenarios that ask for it: libusdr requests READY
/// to rewrite the power-down registers, which stops every Si5332 output; rsl-usdr flips
/// `OUT3_OE` (0xB6 bit 6) while ACTIVE. The block must be exactly libusdr's six writes for
/// an RX-only device, so nothing else can hide in the cut.
fn with_band_crossing_fix(window: &[Op]) -> (Vec<Op>, bool) {
    let clock_write = |bytes: [u8; 2]| Op::I2c {
        addr: CLOCK,
        write: bytes.to_vec(),
        read: Vec::new(),
    };
    let Some(start) = window
        .iter()
        .position(|op| matches!(op, Op::I2c { addr, write, .. } if *addr == CLOCK && write.first() == Some(&0xb6)))
    else {
        return (window.to_vec(), false);
    };
    let on = matches!(&window[start], Op::I2c { write, .. } if write[..] == [0xb6, 0x43]);
    // OUT3 and OUT0/1 enabled (TX not running); READY; power-downs; ACTIVE.
    let libusdr_block = if on {
        [
            [0xb6, 0x43],
            [0x06, 0x01],
            [0xba, 0x76],
            [0xbb, 0x34],
            [0xbc, 0x08],
            [0x06, 0x02],
        ]
    } else {
        [
            [0xb6, 0x03],
            [0x06, 0x01],
            [0xba, 0x7e],
            [0xbb, 0x3c],
            [0xbc, 0x48],
            [0x06, 0x02],
        ]
    };
    let block = window
        .get(start..start + libusdr_block.len())
        .expect("libusdr's set_port3_en block is complete");
    assert_eq!(
        block,
        libusdr_block.map(clock_write),
        "libusdr's set_port3_en block is as expected"
    );
    let fixed = clock_write([0xb6, if on { 0xff } else { 0xbf }]);
    let fixed_window = [
        &window[..start],
        &[fixed],
        &window[start + libusdr_block.len()..],
    ]
    .concat();
    (fixed_window, true)
}

/// Makes `calls` in turn on fresh boards from `board` through libusdr and rsl-usdr,
/// compares each call's outcome and operations, and returns whether each succeeded.
/// [`assert_calls_succeed`] is the usual form.
fn assert_calls_match(board: impl Fn() -> SimBoard, calls: &[Call]) -> Vec<bool> {
    assert_calls_match_crossing(board, calls, Crossing::None)
}

/// As [`assert_calls_match`], with libusdr's band crossings replaced as `crossing` says.
fn assert_calls_match_crossing(
    board: impl Fn() -> SimBoard,
    calls: &[Call],
    crossing: Crossing,
) -> Vec<bool> {
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
    let mut crossings = 0;
    for ((call, (expected_ok, expected)), (result, actual)) in
        calls.iter().zip(expected).zip(actual)
    {
        assert_eq!(
            result.is_ok(),
            expected_ok,
            "{call:?}: libusdr ok={expected_ok}, rsl-usdr {result:?}"
        );
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
        outcomes.push(expected_ok);
    }
    if crossing == Crossing::Fixed {
        assert!(
            crossings > 0,
            "the scenario crosses the mixer band at least once"
        );
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

/// As [`assert_calls_succeed`], for scenarios that cross into or out of the mixer band.
fn assert_crossings_succeed(board: impl Fn() -> SimBoard, calls: &[Call]) {
    let outcomes = assert_calls_match_crossing(board, calls, Crossing::Fixed);
    assert!(
        outcomes.iter().all(|&ok| ok),
        "every call succeeds: {calls:?} -> {outcomes:?}"
    );
}

/// A revision-3 board.
fn rev3() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3)
}

/// A fixed bandwidth survives rate changes; 0 writes the narrowest filter and hands the
/// filter back to the rate.
#[test]
fn fixed_bandwidth_holds_across_rate_changes_like_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Bandwidth(5_000_000),
            Call::Rate(10_000_000),
            Call::Bandwidth(0),
            Call::Rate(20_000_000),
        ],
    );
}

/// Above 47 MHz the filter is bypassed.
#[test]
fn wide_bandwidth_bypasses_the_filter_like_libusdr() {
    assert_calls_succeed(rev3, &[Call::Rate(20_000_000), Call::Bandwidth(50_000_000)]);
}

/// Each band above the mixer's: LNA1 at three VCO ranges, then LNA2.
#[test]
fn tuning_across_lna1_and_lna2_matches_libusdr() {
    assert_calls_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(1_000_000_000),
            Call::Frequency(433_000_000),
            Call::Frequency(2_400_000_000),
            Call::Frequency(3_000_000_000),
        ],
    );
}

#[test]
fn rev1_tuning_matches_libusdr() {
    assert_calls_succeed(
        || SimBoard::new(BoardRevision::Rev1),
        &[Call::Rate(1_000_000), Call::Frequency(915_000_000)],
    );
}

#[test]
fn rev2_tuning_matches_libusdr() {
    assert_calls_succeed(
        || SimBoard::new(BoardRevision::Rev2),
        &[Call::Rate(1_000_000), Call::Frequency(915_000_000)],
    );
}

/// Below 230 MHz the board mixer and Si5332 output 3 come on and the PLL tunes above the
/// signal by the mixer LO; a rate change moves the mixer LO and retunes; leaving the band
/// turns the mixer and output 3 off again. Band crossings use the fix.
#[test]
fn the_mixer_band_matches_libusdr() {
    assert_crossings_succeed(
        rev3,
        &[
            Call::Rate(20_000_000),
            Call::Frequency(100_000_000),
            Call::Rate(10_000_000),
            Call::Frequency(1_000_000_000),
        ],
    );
}

/// Revisions 1 and 2 route the RX clock through output 0, not 1.
#[test]
fn rev1_mixer_band_matches_libusdr() {
    assert_crossings_succeed(
        || SimBoard::new(BoardRevision::Rev1),
        &[Call::Rate(1_000_000), Call::Frequency(100_000_000)],
    );
}

/// A board whose RX PLL locks only from 245 MHz up.
fn rev3_unlocked_below_245m() -> SimBoard {
    SimBoard::new(BoardRevision::Rev3).with_rx_pll_lock(PllLock {
        unlocked_below_hz: 245_000_000,
        ..PllLock::default()
    })
}

/// 240 MHz cannot lock: libusdr retries, steps up to 245 MHz and covers the 5 MHz with the
/// NCO, which 1 MS/s (a 32 MS/s ADC) has room for; the next low tune reuses 245 MHz.
#[test]
fn the_low_lo_fallback_matches_libusdr() {
    assert_calls_succeed(
        rev3_unlocked_below_245m,
        &[
            Call::Rate(1_000_000),
            Call::Frequency(240_000_000),
            Call::Frequency(241_000_000),
            Call::Rate(2_000_000),
            Call::Frequency(1_000_000_000),
        ],
    );
}

/// At 20 MS/s (a 40 MS/s ADC) the 5 MHz NCO offset does not fit, so tuning fails on both
/// sides, after the same operations.
#[test]
fn a_fallback_without_nco_room_fails_like_libusdr() {
    let outcomes = assert_calls_match(
        rev3_unlocked_below_245m,
        &[Call::Rate(20_000_000), Call::Frequency(240_000_000)],
    );
    assert_eq!(outcomes, [true, false]);
}

/// Above 250 MHz there is no fallback: a PLL that cannot lock fails the call.
#[test]
fn an_unlockable_lo_above_250m_fails_like_libusdr() {
    let outcomes = assert_calls_match(
        || {
            SimBoard::new(BoardRevision::Rev3).with_rx_pll_lock(PllLock {
                unlocked_below_hz: 2_000_000_000,
                ..PllLock::default()
            })
        },
        &[Call::Rate(20_000_000), Call::Frequency(1_000_000_000)],
    );
    assert_eq!(outcomes, [true, false]);
}

/// 231 MHz needs a 14 MHz NCO offset: 1 MS/s plus 14 MHz is 15 MHz, just over 45% of the
/// 32 MS/s ADC (14.4 MHz), so libusdr refuses it.
#[test]
fn the_nco_headroom_limit_matches_libusdr() {
    let outcomes = assert_calls_match(
        rev3_unlocked_below_245m,
        &[Call::Rate(1_000_000), Call::Frequency(231_000_000)],
    );
    assert_eq!(outcomes, [true, false]);
}
