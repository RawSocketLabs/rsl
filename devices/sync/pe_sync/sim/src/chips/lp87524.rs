//! The LP87524 PMIC: I2C core 1, bus 0, address 0x60 (`I2C_BUS_LP87524` in `pe_sync.c`),
//! with an 8-bit register pointer. libusdr only reads register 0x01 (logged as
//! `LP87524_OTP`) during its status check.

use super::i2c::{I2cRegisters, Reset};

/// The one register the pinned `hw/lp87524j/lp87524j.yaml` documents: `BUCK1_CTRL1` (0x04).
/// Assumption: every other register, 0x01 included, resets to 0; the OTP revision is not in
/// the vendored source.
const RESET: [Reset; 1] = [(0x04, 0xc4, 0x00)];

/// An LP87524 fresh out of power-on reset.
pub(crate) fn lp87524() -> I2cRegisters {
    I2cRegisters::new(1, &RESET)
}
