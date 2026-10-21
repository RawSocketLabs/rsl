//! pe-sync's open and close against libusdr's, operation for operation, on fresh
//! simulated boards: the whole session the characterization golden records, and the
//! refusals that power the board off again; and boards whose PLLs lock slowly or never,
//! which libusdr's lock waits time by the shim's virtual wall clock.
//!
//! No divergence is whitelisted: every scenario here matches libusdr exactly.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use pe_sync::{Device, Error};
use pe_sync_oracle::Oracle;
use pe_sync_sim::{I2cAddress, Lock, Op, Pll, SimBoard};
use sim_bus::SimBus;

/// `ENODEV`: `lmk05318_create`'s refusal of a wrong identity.
const ENODEV: i32 = 19;
/// `EINVAL`: `lmk1d1208i_create`'s refusal of a wrong identity.
const EINVAL: i32 = 22;
/// `ETIMEDOUT`: every lock wait's timeout.
const ETIMEDOUT: i32 = 110;
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

/// Every PLL the open waits for, with the name pe-sync's `Error::Unlocked` gives it.
const PLLS: [(Pll, &str); 5] = [
    (Pll::Apll1, "LMK05318 APLL1"),
    (Pll::Apll2, "LMK05318 APLL2"),
    (Pll::Lmx2820_0, "LMX2820"),
    (Pll::Lmx2820_1, "LMX2820"),
    (Pll::Lmx1204, "LMX1204"),
];

#[test]
fn a_pll_that_never_locks_times_out_and_powers_off_like_libusdr() {
    // Each wait polls for its whole timeout (libusdr's by the shim's virtual wall clock),
    // then the open stops, logs its status reads and powers the board off.
    for (pll, name) in PLLS {
        let scenario = format!("{pll:?} never locks");
        let board = || SimBoard::new().with_lock(pll, Lock::Never);
        let refused = refused_like_libusdr(board, ETIMEDOUT, &scenario);
        assert!(
            matches!(refused, Error::Unlocked(chip) if chip == name),
            "{scenario}: {refused:?}"
        );
    }
}

#[test]
fn a_slow_lock_repeats_its_polls_and_opens_like_libusdr() {
    let (healthy, _) = rsl_session(SimBoard::new());
    for (pll, _) in PLLS {
        let scenario = format!("{pll:?} locks after 5 polls");
        let board = || SimBoard::new().with_lock(pll, Lock::AfterReads(5));
        let (libusdr_open, libusdr_close) = libusdr_session(board());
        let (open, close) = rsl_session(board());
        assert!(
            open.len() > healthy.len(),
            "{scenario}: the wait polled again"
        );
        assert_same_ops(&libusdr_open, &open, &scenario);
        assert_same_ops(&libusdr_close, &close, &scenario);
    }
}

#[test]
fn reopening_after_a_lock_timeout_matches_libusdr() {
    // The LMX1204 stays unlocked through the first open's 1000 polls and its status
    // reads, and locks during the second open's wait.
    let board = || SimBoard::new().with_lock(Pll::Lmx1204, Lock::AfterReads(1500));
    let failed = Oracle::open(board())
        .err()
        .expect("libusdr's first open times out");
    let oracle = Oracle::open(*failed.board).expect("libusdr's second open locks");
    let libusdr = oracle.close();

    let sim = SimBus::new(board());
    let refused = Device::open(sim.clone()).err();
    assert!(
        matches!(refused, Some(Error::Unlocked("LMX1204"))),
        "{refused:?}"
    );
    let device = Device::open(sim.clone()).expect("pe-sync's second open locks");
    device.close().expect("pe-sync closes the board");
    assert_same_ops(libusdr.trace(), sim.board().trace(), "reopen");
}
