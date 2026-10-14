//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable before 1.0.** This mirrors libusdr's `lowlevel_ops.ls_op` so custom buses,
//! including the board simulator, can drive a [`Device`](crate::Device). The seam itself is
//! the shared `rsl-lowlevel` crate's, re-exported here. On the uSDR:
//!
//! - I2C uses core 0 only; the transports refuse any other [`I2cAddr::core`].
//! - RX streaming ([`Bus::rx_stream_open`]): `PCIe` uses 32 DMA buffers; USB queues eight
//!   transfers with additional trailer space.
//! - Of [`Bus::rx_stream_recv`]'s out-of-band words, the driver uses only the lost packet
//!   count.
//! - TX streaming ([`Bus::tx_stream_open`]): `PCIe` uses 32 DMA buffers. The status words
//!   [`Bus::tx_stream_send`] lends with a buffer are, over `PCIe`, the driver's completion
//!   record (the TX core's three event words and the kernel's nanosecond time), none for
//!   the 32 buffers it starts with; libusdr's USB transport reads registers 28 to 31 (not
//!   ported). The driver announces each block through the TX core's registers 12 and 13.

use std::io;

pub use rsl_lowlevel::{
    Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr, TxCommit, TxFill,
};

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
/// `rsl-lowlevel` stays independent of `usb`.
#[cfg(all(feature = "usb", target_os = "linux"))]
#[must_use]
pub fn usb_error(error: usb::Error) -> BusError {
    match error {
        usb::Error::TimedOut => BusError::Timeout,
        usb::Error::Disconnected => BusError::Disconnected,
        usb::Error::Io(error) => BusError::Io(error),
        other => BusError::Other(Box::new(other)),
    }
}
