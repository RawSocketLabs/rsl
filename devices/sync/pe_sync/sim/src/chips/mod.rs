//! Register-level models of the chips on the `PE_SYNC` board: two register-file shapes (TI
//! 24-bit SPI and pointer-addressed I2C) and, per chip, its reset values and readbacks.

mod i2c;
mod lmk05318;
mod lmk1d1208i;
mod lmx1204;
mod lmx1214;
mod lmx2820;
mod lp87524;
mod spi;

pub(crate) use i2c::I2cRegisters;
pub(crate) use lmk1d1208i::lmk1d1208i;
pub(crate) use lmk05318::lmk05318;
pub(crate) use lmx1204::lmx1204;
pub(crate) use lmx1214::lmx1214;
pub(crate) use lmx2820::lmx2820;
pub(crate) use lp87524::lp87524;
pub(crate) use spi::SpiRegisters;
