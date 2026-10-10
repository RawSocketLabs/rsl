//! libusdr's open sequence against a healthy simulated board.

use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, I2cAddress, Op, SimBoard};

/// GPO bank driving the LMS6002D reset line (active low).
const GPO_LMS_RST: u8 = 0;
/// GPO bank enabling the RX/TX booster.
const GPO_BOOSTER: u8 = 7;
/// The Si5332 clock generator.
const CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };
/// The LP8758 PMIC.
const PMIC: I2cAddress = I2cAddress { bus: 0, addr: 0x60 };

/// Position of the first trace entry matching `pred`.
fn position(trace: &[Op], what: &str, pred: impl Fn(&Op) -> bool) -> usize {
    trace
        .iter()
        .position(pred)
        .unwrap_or_else(|| panic!("trace has no {what}"))
}

#[test]
fn rev3_opens_and_brings_up_power_then_clock_then_rf() {
    let oracle =
        Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens a healthy board");
    let board = oracle.close();
    let trace = board.trace();

    let pmic_enable = position(
        trace,
        "PMIC BUCK0 enable",
        |op| matches!(op, Op::I2c { addr, write, .. } if *addr == PMIC && write.first() == Some(&0x02)),
    );
    let clock_active = position(
        trace,
        "Si5332 ACTIVE",
        |op| matches!(op, Op::I2c { addr, write, .. } if *addr == CLOCK && write[..] == [0x06, 0x02]),
    );
    let lms_out_of_reset = position(
        trace,
        "LMS reset release",
        |op| matches!(op, Op::RegWrite { addr: 0, value } if *value == u32::from(GPO_LMS_RST) << 24 | 1),
    );
    let lms_id_read = position(trace, "LMS version read", |op| {
        matches!(op, Op::Spi { out: 0x0400, .. })
    });

    assert!(pmic_enable < clock_active, "rails before clocks");
    assert!(
        clock_active < lms_out_of_reset,
        "clocks before RF reset release"
    );
    assert!(
        lms_out_of_reset < lms_id_read,
        "RF chip read only out of reset"
    );
    assert!(
        board.now_us() >= 21_000,
        "bring-up sleeps at least 21 ms, slept {} us",
        board.now_us()
    );
    // Closing powers the board down again.
    assert_eq!(board.gpo(GPO_LMS_RST), Some(0));
    assert_eq!(board.gpo(GPO_BOOSTER), Some(0));
}
