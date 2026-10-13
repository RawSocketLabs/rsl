//! FPGA bus identities and bounds: `rsl-lowlevel`'s, under the portable tree's path.

/// The USB machines' bounds.
#[cfg(feature = "usb")]
pub(crate) use rsl_lowlevel::{I2C_MAX_READ, I2C_MAX_WRITE};
pub(crate) use rsl_lowlevel::{I2cAddr, SpiAddr};
