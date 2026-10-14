//! usdr's identity query against libusdr's `/dm/revision`, value and operations.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use sim_bus::SimBus;
use usdr::Device;
use usdr_oracle::Oracle;
use usdr_sim::{BoardRevision, Op, SimBoard};

fn assert_identity_matches(revision: BoardRevision) {
    let mut oracle = Oracle::open(SimBoard::new(revision)).expect("libusdr opens the board");
    let start = oracle.trace_len();
    let expected_value = oracle
        .get_uint(c"/dm/revision")
        .expect("libusdr reports the revision");
    let end = oracle.trace_len();
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(revision));
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up the board");
    let actual_start = sim.board().trace().len();
    let identity = device.identity().expect("usdr reads its identity");
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

/// Makes a fresh board of one kind.
type NewBoard = fn() -> SimBoard;

#[test]
fn channel_capabilities_match_libusdr_for_each_chain() {
    let boards: [(&str, NewBoard); 4] = [
        ("both chains", || SimBoard::new(BoardRevision::Rev3)),
        ("RX only", || {
            SimBoard::new(BoardRevision::Rev3).with_rx_only()
        }),
        ("TX only", || {
            SimBoard::new(BoardRevision::Rev3).with_tx_only()
        }),
        ("extended TX core", || {
            SimBoard::new(BoardRevision::Rev3).with_extended_tx_core()
        }),
    ];
    for (name, board) in boards {
        let mut oracle = Oracle::open(board()).expect("libusdr opens the board");
        let mut get = |path| oracle.get_uint(path).expect("libusdr answers");
        let expected = [
            get(c"/ll/sdr/max_sw_rx_chans"),
            get(c"/ll/sdr/max_sw_tx_chans"),
            get(c"/ll/sdr/max_hw_rx_chans"),
            get(c"/ll/sdr/max_hw_tx_chans"),
            get(c"/dm/revision") >> 58 & 1,
        ];
        let _ = oracle.close();

        let sim = SimBus::new(board());
        let mut device = Device::with_bus(sim).expect("usdr powers up the board");
        let identity = device.identity().expect("usdr reads its identity");
        let actual = [
            identity.max_rx_channels().into(),
            identity.max_tx_channels().into(),
            1,
            1,
            identity.has_extended_tx_core().into(),
        ];
        assert_eq!(actual, expected, "{name}: channels and TX core");
    }
}
