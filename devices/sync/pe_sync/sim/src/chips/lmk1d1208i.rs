//! The LMK1D1208I LVDS fan-out buffer (`hw/lmk1d1208i/lmk1d1208i.c`): two on I2C core 0,
//! bus 1, at 0x68 (`I2C_BUS_LMK1D1208I_LCK`) and 0x69 (`I2C_BUS_LMK1D1208I_LRF`), with an
//! 8-bit register pointer.

use super::i2c::{I2cRegisters, Reset};

/// The documented registers' reset values and read-only masks, from the `dflt` and `mode`
/// of each field in the pinned `hw/lmk1d1208i/lmk1d1208i.yaml`. `R5` (0x05) resets to
/// `REV_ID` 2, `DEV_ID` 0, which `lmk1d1208i_create` accepts.
const RESET: [Reset; 5] = [
    (0x00, 0x00, 0x00),
    (0x01, 0x00, 0x00),
    (0x02, 0xf1, 0x00),
    (0x05, 0x20, 0xff),
    (0x0e, 0x00, 0xff),
];

/// An LMK1D1208I fresh out of power-on reset.
pub(crate) fn lmk1d1208i() -> I2cRegisters {
    I2cRegisters::new(1, &RESET)
}
