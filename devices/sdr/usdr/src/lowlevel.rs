//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable before 1.0.** This mirrors libusdr's `lowlevel_ops.ls_op` so custom buses,
//! including the board simulator, can drive a [`Device`](crate::Device). The seam itself is
//! the shared `rsl-lowlevel` crate's, re-exported here; the uSDR uses I2C core 0 only.

use std::io;

pub use rsl_lowlevel::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};

use crate::portable::error::Failure;

/// Interprets portable failures at the host boundary, preserving original transport sources.
impl From<Failure<BusError>> for BusError {
    fn from(error: Failure<BusError>) -> Self {
        match error {
            #[cfg(feature = "usb")]
            Failure::Timeout => Self::Timeout,
            #[cfg(feature = "usb")]
            Failure::Unsupported(reason) => Self::Unsupported(reason),
            Failure::InvalidData(reason) => {
                Self::Io(io::Error::new(io::ErrorKind::InvalidData, reason))
            }
            Failure::Transport(error) => error,
        }
    }
}

/// A USB host failure as the bus failure the uSDR's USB transport reports: timeouts and
/// disconnects keep their variants, I/O errors their source, anything else is
/// [`BusError::Other`].
///
/// A function, not `From` (which this replaces): both types are foreign here, and
/// `rsl-lowlevel` stays independent of `rsl-usb`.
#[cfg(all(feature = "usb", target_os = "linux"))]
#[must_use]
pub fn usb_error(error: rsl_usb::Error) -> BusError {
    match error {
        rsl_usb::Error::TimedOut => BusError::Timeout,
        rsl_usb::Error::Disconnected => BusError::Disconnected,
        rsl_usb::Error::Io(error) => BusError::Io(error),
        other => BusError::Other(Box::new(other)),
    }
}
