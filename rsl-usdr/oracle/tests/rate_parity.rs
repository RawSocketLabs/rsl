//! rsl-usdr's RX sample-rate changes against libusdr's, operation for operation.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

/// Sets each of `rates` in turn on fresh boards through libusdr and rsl-usdr, and compares
/// the operations of every call.
fn assert_rate_changes_match(revision: BoardRevision, rates: &[u32]) {
    assert_rate_changes_match_on(revision, || SimBoard::new(revision), rates);
}

/// As [`assert_rate_changes_match`], on boards made by `board`.
fn assert_rate_changes_match_on(
    revision: BoardRevision,
    board: impl Fn() -> SimBoard,
    rates: &[u32],
) {
    let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
    let mut expected_windows: Vec<Range<usize>> = Vec::new();
    for &rate in rates {
        let start = oracle.trace_len();
        oracle.set_rx_rate(rate).expect("libusdr accepts the rate");
        expected_windows.push(start..oracle.trace_len());
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let mut actual_windows: Vec<Range<usize>> = Vec::new();
    for &rate in rates {
        let start = sim.board().trace().len();
        device
            .set_rx_sample_rate(rate)
            .expect("rsl-usdr accepts the rate");
        actual_windows.push(start..sim.board().trace().len());
    }
    let board = sim.board();

    for ((rate, expected), actual) in rates.iter().zip(expected_windows).zip(actual_windows) {
        let expected: &[Op] = &reference.trace()[expected];
        let actual: &[Op] = &board.trace()[actual];
        if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
            panic!(
                "{revision:?} at {rate} S/s: first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
                expected[i], actual[i]
            );
        }
        assert_eq!(
            expected.len(),
            actual.len(),
            "{revision:?} at {rate} S/s: same prefix, different lengths"
        );
    }
}

#[test]
fn rev1_rate_changes_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev1, &[1_000_000]);
}

#[test]
fn rev2_rate_changes_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev2, &[20_000_000]);
}

/// ×32, ×2 and ×1 decimation; the I/O rail boosted at 62 MS/s and lowered again; the RX
/// filter from its narrowest setting to bypassed.
#[test]
fn rev3_rate_changes_match_libusdr() {
    assert_rate_changes_match(
        BoardRevision::Rev3,
        &[1_000_000, 20_000_000, 62_000_000, 20_000_000],
    );
}

/// 10 and 15 MS/s both decimate by 4, so the second change reloads no FIR.
#[test]
fn rev3_same_decimation_skips_the_fir_like_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[10_000_000, 15_000_000]);
}

/// 1.002 MS/s exercises the Si5332 denominator search's 32-bit wrap.
#[test]
fn rev3_wrapped_pll_denominator_matches_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[1_002_000]);
}

/// x8 at 4 MS/s; x16 at 1.9 MS/s, where x32 would overshoot 60 MS/s and libusdr steps back.
/// With the tests above, every RX FIR table passes through the comparison.
#[test]
fn rev3_remaining_decimations_match_libusdr() {
    assert_rate_changes_match(BoardRevision::Rev3, &[4_000_000, 1_900_000]);
}

/// Without a TX chain libusdr never decimates, so the sample clock runs at twice the
/// requested rate: 2 MHz (slowest slew), 26 MHz (the reference itself, so the outputs
/// bypass the PLL, slew 2) and 160 MHz (fastest slew, I/O rail boosted).
#[test]
fn rev3_rx_only_rate_changes_match_libusdr() {
    assert_rate_changes_match_on(
        BoardRevision::Rev3,
        || SimBoard::new(BoardRevision::Rev3).with_rx_only(),
        &[1_000_000, 13_000_000, 80_000_000],
    );
}
