//! RFIC power-up and close ordering, from `xsdr_pwren` and `xsdr_dtor`.

use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// GPO bank `IGPO_LMS_PWR`.
const GPO_LMS_PWR: u8 = 0;
/// `IGPO_LDOLMS_EN`.
const GPO_LDOLMS_EN: u8 = 17;
/// `IGPO_LMS_PWR` (bank 0) = `LDOEN | NRESET`: supplies on, reset released.
const LMS_RUNNING: Op = Op::RegWrite {
    addr: 0,
    value: 0b1001,
};

/// Powers `variant` up and closes it, returning the board.
fn power_cycle(variant: Variant) -> SimBoard {
    let mut oracle = Oracle::open(SimBoard::new(variant)).expect("libusdr opens the board");
    oracle.power_up().expect("libusdr powers the RFIC up");
    oracle.close()
}

/// The LMS7002M is spoken to only once its reset is released, and close leaves it held in
/// reset with its supplies off.
fn assert_rfic_power_cycle(variant: Variant) {
    let board = power_cycle(variant);
    let trace = board.trace();
    let released = trace
        .iter()
        .position(|op| *op == LMS_RUNNING)
        .expect("power-up releases the LMS7002M reset");
    let first_spi = trace
        .iter()
        .position(|op| matches!(op, Op::Spi { .. }))
        .expect("power-up talks to the LMS7002M");
    assert!(released < first_spi, "no SPI before reset release");
    assert_eq!(
        board.lms7_reg(0x0082) & 0b11_1110,
        0b11_1110,
        "the AFE stays powered down"
    );
    assert_eq!(board.gpo(GPO_LMS_PWR), Some(0), "close resets the LMS7002M");
    assert_eq!(
        board.gpo(GPO_LDOLMS_EN),
        Some(0),
        "close turns its LDOs off"
    );
}

#[test]
fn xsdr_rfic_power_cycle_releases_reset_before_spi_and_ends_off() {
    assert_rfic_power_cycle(Variant::Xsdr);
}

#[test]
fn xtrx_rfic_power_cycle_releases_reset_before_spi_and_ends_off() {
    assert_rfic_power_cycle(Variant::Xtrx);
}
