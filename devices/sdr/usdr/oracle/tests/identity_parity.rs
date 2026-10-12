//! rsl-usdr's identity query against libusdr's `/dm/revision`, value and operations.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};
use sim_bus::SimBus;

fn assert_identity_matches(revision: BoardRevision) {
    let mut oracle = Oracle::open(SimBoard::new(revision)).expect("libusdr opens the board");
    let start = oracle.trace_len();
    let expected_value = oracle
        .get_uint(c"/dm/revision")
        .expect("libusdr reports the revision");
    let end = oracle.trace_len();
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(revision));
    let mut device = Device::with_bus(sim.clone()).expect("rsl-usdr powers up the board");
    let actual_start = sim.board().trace().len();
    let identity = device.identity().expect("rsl-usdr reads its identity");
    let board = sim.board();

    assert_eq!(identity.to_raw(), expected_value, "{revision:?}: value");
    let expected: &[Op] = &reference.trace()[start..end];
    let actual: &[Op] = &board.trace()[actual_start..];
    assert_eq!(expected, actual, "{revision:?}: operations");
}

#[test]
fn rev1_identity_matches_libusdr() {
    assert_identity_matches(BoardRevision::Rev1);
}

#[test]
fn rev2_identity_matches_libusdr() {
    assert_identity_matches(BoardRevision::Rev2);
}

#[test]
fn rev3_identity_matches_libusdr() {
    assert_identity_matches(BoardRevision::Rev3);
}
