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

/// Power-cycles fresh copies of a board through libusdr and rsl-usdr and compares traces.
fn assert_power_cycle_matches(board: impl Fn() -> SimBoard) {
    let reference = Oracle::open(board())
        .expect("libusdr opens the board")
        .close();

    let sim = SimBus::new(board());
    Device::with_bus(sim.clone())
        .expect("rsl-usdr powers up the board")
        .close()
        .expect("rsl-usdr powers down");

    let expected = without_frontend_probe(reference.trace());
    let actual = sim.board().trace().to_vec();
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
    assert_power_cycle_matches(|| SimBoard::new(BoardRevision::Rev1));
}

#[test]
fn rev2_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(|| SimBoard::new(BoardRevision::Rev2));
}

#[test]
fn rev3_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(|| SimBoard::new(BoardRevision::Rev3));
}

/// The clock generator reports no input until the oscillator is enabled after it is
/// programmed; libusdr carries on, and so must rsl-usdr.
#[test]
fn rev3_cold_oscillator_power_cycle_matches_libusdr() {
    assert_power_cycle_matches(|| SimBoard::new(BoardRevision::Rev3).with_oscillator_off());
}
