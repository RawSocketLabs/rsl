//! rsl-usdr's power-up and power-down against libusdr's, operation for operation.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_usdr::Device;
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, I2cAddress, Op, SimBoard};
use sim_bus::SimBus;

/// Operations libusdr's `close` performs: LMS reset, LED and booster off.
const POWER_DOWN_OPS: usize = 3;

/// libusdr's trace without its front-end probe.
///
/// Intentional divergence, scoped to this scenario: after RF init libusdr probes for
/// optional front-end boards (`device_fe_probe` in `m2_lm6_1_initialize`), finds none, and
/// ignores the result. rsl-usdr supports no front ends, so it does not probe. The probe
/// may only write FPGA register 3, sleep, and talk on I2C bus 1 (the front-end
/// connector, `I2C_BUS_FRONTEND`), so nothing else can hide in the cut.
fn without_frontend_probe(trace: &[Op]) -> Vec<Op> {
    let dc_correction_on = Op::RegWrite {
        addr: 0,
        value: 9 << 24 | 1,
    };
    let end_of_init = trace
        .iter()
        .position(|op| *op == dc_correction_on)
        .expect("libusdr enables DC correction last")
        + 1;
    let power_down = trace.len() - POWER_DOWN_OPS;
    let probe = &trace[end_of_init..power_down];
    assert!(
        probe.iter().all(|op| matches!(
            op,
            Op::RegWrite { addr: 3, .. }
                | Op::Sleep { .. }
                | Op::I2c {
                    addr: I2cAddress { bus: 1, .. },
                    ..
                }
        )),
        "front-end probe reached beyond register 3 and I2C bus 1: {probe:x?}"
    );
    [&trace[..end_of_init], &trace[power_down..]].concat()
}

/// The TMP114.
const TEMP: I2cAddress = I2cAddress { bus: 0, addr: 0x4e };
/// The LP8758 PMIC.
const PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };

/// rsl-usdr's trace without its thermal gate.
///
/// Intentional addition: before the first PMIC operation rsl-usdr reads the TMP114
/// temperature register, and on revisions 1 and 2 first checks the TMP114 ID (libusdr
/// checks it only on revision 3). The gate must sit exactly there, so a misplaced or
/// extra temperature read still fails the comparison.
fn without_thermal_gate(trace: &[Op], revision: BoardRevision) -> Vec<Op> {
    let gate_len = if revision == BoardRevision::Rev3 {
        1
    } else {
        2
    };
    let first_pmic_op = trace
        .iter()
        .position(|op| matches!(op, Op::I2c { addr, .. } if *addr == PMIC))
        .expect("power-up reaches the PMIC");
    let gate = &trace[first_pmic_op - gate_len..first_pmic_op];
    assert!(
        matches!(gate.last(), Some(Op::I2c { addr, write, .. }) if *addr == TEMP && write[..] == [0x00]),
        "the gate ends in one temperature read: {gate:x?}"
    );
    assert!(
        gate.iter()
            .all(|op| matches!(op, Op::I2c { addr, .. } if *addr == TEMP)),
        "the gate only talks to the TMP114: {gate:x?}"
    );
    [&trace[..first_pmic_op - gate_len], &trace[first_pmic_op..]].concat()
}

/// Power-cycles fresh copies of a board through libusdr and rsl-usdr and compares traces.
fn assert_power_cycle_matches(revision: BoardRevision, board: impl Fn() -> SimBoard) {
    let reference = Oracle::open(board())
        .expect("libusdr opens the board")
        .close();

    let sim = SimBus::new(board());
    Device::with_bus(sim.clone())
        .expect("rsl-usdr powers up the board")
        .close()
        .expect("rsl-usdr powers down");

    let expected = without_frontend_probe(reference.trace());
    let actual = without_thermal_gate(sim.board().trace(), revision);
    if let Some(i) = expected.iter().zip(&actual).position(|(e, a)| e != a) {
        panic!(
            "first difference at op {i}: libusdr {:x?}, rsl-usdr {:x?}",
            expected[i], actual[i]
        );
    }
    assert_eq!(
        expected.len(),
        actual.len(),
        "same prefix, different lengths"
    );
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
