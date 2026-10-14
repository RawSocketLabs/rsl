//! pe-sync's open and close against libusdr's, operation for operation, on fresh
//! simulated boards: the whole session the characterization golden records, and the
//! refusals that power the board off again.
//!
//! No divergence is whitelisted: every scenario here matches libusdr exactly.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use pe_sync::{Device, Error};
use pe_sync_oracle::Oracle;
use pe_sync_sim::{I2cAddress, Op, SimBoard};
use sim_bus::SimBus;

/// `ENODEV`: `lmk05318_create`'s refusal of a wrong identity.
const ENODEV: i32 = 19;
/// `EINVAL`: `lmk1d1208i_create`'s refusal of a wrong identity.
const EINVAL: i32 = 22;
/// The LMK05318B (`I2C_BUS_LMK05318B`).
const LMK05318: I2cAddress = I2cAddress {
    core: 0,
    bus: 0,
    addr: 0x64,
};
/// The `LCK` and `LRF` LMK1D1208I fan-out buffers.
const LMK1D1208I: [I2cAddress; 2] = [
    I2cAddress {
        core: 0,
        bus: 1,
        addr: 0x68,
    },
    I2cAddress {
        core: 0,
        bus: 1,
        addr: 0x69,
    },
];
/// The LP87524 PMIC (`I2C_BUS_LP87524`), the only chip on I2C core 1.
const LP87524: I2cAddress = I2cAddress {
    core: 1,
    bus: 0,
    addr: 0x60,
};

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: pe-sync diverges from libusdr at op {at} of {}: libusdr {}, \
             pe-sync {}",
            expected.len(),
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// libusdr's open and close of `board`, split at the end of the open.
fn libusdr_session(board: SimBoard) -> (Vec<Op>, Vec<Op>) {
    let oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let board = oracle.close();
    let (open, close) = board.trace().split_at(opened);
    (open.to_vec(), close.to_vec())
}

/// pe-sync's open and close of `board`, split at the end of the open.
fn rsl_session(board: SimBoard) -> (Vec<Op>, Vec<Op>) {
    let sim = SimBus::new(board);
    let device = Device::open(sim.clone()).expect("pe-sync opens the board");
    let opened = sim.board().trace().len();
    device.close().expect("pe-sync closes the board");
    let board = sim.board();
    let (open, close) = board.trace().split_at(opened);
    (open.to_vec(), close.to_vec())
}

#[test]
fn a_pe_sync_session_matches_libusdr_operation_for_operation() {
    // Power-on and its LED fades, the status check, the LMK05318B's bring-up, lock waits
    // and sync, both LMX2820s, the LMX1214, the LMX1204 with its multiplier lock, both
    // LMK1D1208Is; then the close's power-off and fade.
    let (libusdr_open, libusdr_close) = libusdr_session(SimBoard::new());
    let (open, close) = rsl_session(SimBoard::new());
    assert_same_ops(&libusdr_open, &open, "open");
    assert_same_ops(&libusdr_close, &close, "close");
}

#[test]
fn a_silent_pmic_changes_nothing_but_the_byte_both_log() {
    // The LP87524 is only read and logged: an idle bus (0xff) fails nothing.
    let board = || SimBoard::new().without(LP87524);
    let (libusdr_open, _) = libusdr_session(board());
    let (open, _) = rsl_session(board());
    assert_same_ops(&libusdr_open, &open, "open without the LP87524");
}

/// Runs both drivers on `board`, which libusdr refuses with `errno`, and compares
/// everything each did, the power-off after the refusal included; returns pe-sync's
/// error.
fn refused_like_libusdr(board: impl Fn() -> SimBoard, errno: i32, scenario: &str) -> Error {
    let libusdr = Oracle::open(board())
        .err()
        .unwrap_or_else(|| panic!("{scenario}: libusdr refuses the board"));
    assert_eq!(libusdr.errno, -errno, "{scenario}: libusdr's errno");
    let sim = SimBus::new(board());
    let refused = Device::open(sim.clone())
        .err()
        .unwrap_or_else(|| panic!("{scenario}: pe-sync refuses the board"));
    assert_same_ops(libusdr.board.trace(), sim.board().trace(), scenario);
    refused
}

#[test]
fn a_board_without_its_lmk05318_is_refused_and_powered_off_like_libusdr() {
    let refused = refused_like_libusdr(|| SimBoard::new().without(LMK05318), ENODEV, "no LMK05318");
    assert!(matches!(
        refused,
        Error::ChipId {
            chip: "LMK05318",
            ..
        }
    ));
}

#[test]
fn a_board_without_a_fan_out_buffer_is_refused_and_powered_off_like_libusdr() {
    for (index, buffer) in LMK1D1208I.into_iter().enumerate() {
        let scenario = format!("no LMK1D1208I #{index}");
        let refused = refused_like_libusdr(|| SimBoard::new().without(buffer), EINVAL, &scenario);
        assert!(
            matches!(
                refused,
                Error::ChipId {
                    chip: "LMK1D1208I",
                    ..
                }
            ),
            "{scenario}: {refused:?}"
        );
    }
}
