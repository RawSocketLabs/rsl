//! FPGA bus identities and bounds: `rsl-lowlevel`'s, under the portable tree's path.

pub(crate) use rsl_lowlevel::I2cAddr;
/// The USB machines' bounds and SPI targets.
#[cfg(feature = "usb")]
pub(crate) use rsl_lowlevel::{I2C_MAX_READ, I2C_MAX_WRITE, SpiAddr};
