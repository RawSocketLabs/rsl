//! rsl-usdr's power-up and power-down against libusdr's, operation for operation.
//!
//! Power-up ends with libusdr's automatic front-end probe. Nothing is attached to these
//! boards' connector, so every board's probe runs until the M.2-to-PCIe breakout, which
//! comes up without a DAC; `frontend_parity.rs` covers the front ends themselves.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;
#[path = "common/thermal_gate.rs"]
mod thermal_gate;

use std::ffi::CStr;

use rsl_usdr::{Device, ReferenceClock};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, SimBoard};
use sim_bus::SimBus;
use thermal_gate::{assert_same_ops, without_thermal_gate};

/// Power-cycles fresh copies of a board through libusdr and rsl-usdr and compares traces.
fn assert_power_cycle_matches(revision: BoardRevision, board: impl Fn() -> SimBoard) {
    assert_power_cycle_matches_with(revision, board, c"", ReferenceClock::Board);
}

/// As [`assert_power_cycle_matches`], opening libusdr with `parameters` and rsl-usdr with
/// the equivalent `clock`.
fn assert_power_cycle_matches_with(
    revision: BoardRevision,
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    clock: ReferenceClock,
) {
    let reference = Oracle::open_with(board(), parameters)
        .expect("libusdr opens the board")
        .close();

    let sim = SimBus::new(board());
    Device::builder(sim.clone())
        .reference_clock(clock)
        .open()
        .expect("rsl-usdr powers up the board")
        .close()
        .expect("rsl-usdr powers down");

    let actual = without_thermal_gate(sim.board().trace(), revision);
    assert_same_ops(reference.trace(), &actual, &format!("{revision:?}"));
}

#[test]
fn rev1_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(BoardRevision::Rev1, || SimBoard::new(BoardRevision::Rev1));
}

#[test]
fn rev2_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(BoardRevision::Rev2, || SimBoard::new(BoardRevision::Rev2));
}

#[test]
fn rev3_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(BoardRevision::Rev3, || SimBoard::new(BoardRevision::Rev3));
}

/// The clock generator reports no input until the oscillator is enabled after it is
/// programmed; libusdr carries on, and so must rsl-usdr.
#[test]
fn rev3_cold_oscillator_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(BoardRevision::Rev3, || {
        SimBoard::new(BoardRevision::Rev3).with_oscillator_off()
    });
}

/// An external reference on clock input 2 (`extclk=1`, `extref`): the Si5332 selects input
/// 2 and, on revision 3, the on-board oscillator stays off.
#[test]
fn external_reference_power_cycles_match_libusdr() {
    for revision in [BoardRevision::Rev1, BoardRevision::Rev3] {
        assert_power_cycle_matches_with(
            revision,
            || SimBoard::new(revision),
            c"extclk=1,extref=40000000",
            ReferenceClock::External { hz: 40_000_000 },
        );
    }
}

/// The on-board oscillator (`extclk=0`), libusdr's default on revision 3 only.
#[test]
fn oscillator_reference_power_cycles_match_libusdr() {
    for revision in [BoardRevision::Rev2, BoardRevision::Rev3] {
        assert_power_cycle_matches_with(
            revision,
            || SimBoard::new(revision),
            c"extclk=0",
            ReferenceClock::Oscillator,
        );
    }
}
