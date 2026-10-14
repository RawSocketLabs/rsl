//! FPGA bus identities and bounds: `hal`'s, under the portable tree's path.

/// The USB machines' bounds.
#[cfg(feature = "usb")]
pub(crate) use hal::{I2C_MAX_READ, I2C_MAX_WRITE};
pub(crate) use hal::{I2cAddr, SpiAddr};
