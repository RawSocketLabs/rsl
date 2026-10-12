//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable before 1.0.** This mirrors libusdr's `lowlevel_ops.ls_op` so custom buses,
//! including the board simulator, can drive a [`Device`](crate::Device). Streaming methods
//! join the trait when streaming is ported.

use std::error::Error as StdError;
use std::io;
use std::time::Duration;

use crate::portable::error::Failure;

pub use crate::portable::address::{I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};

/// A failed bus operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BusError {
    /// The operation did not complete in time.
    #[error("bus operation timed out")]
    Timeout,

    /// The device was unplugged or its driver went away.
    #[error("device disconnected")]
    Disconnected,

    /// The bus cannot perform this operation (for example, an I2C transfer over the limits).
    #[error("unsupported bus operation: {0}")]
    Unsupported(&'static str),

    /// An operating-system error from the transport.
    #[error("bus I/O error")]
    Io(#[from] io::Error),

    /// Any other failure. An escape hatch for custom buses whose errors have no variant
    /// here; the driver's own transports use the specific variants.
    #[error(transparent)]
    Other(Box<dyn StdError + Send + Sync>),
}

/// Interprets portable failures at the host boundary, preserving original transport sources.
impl From<Failure<Self>> for BusError {
    fn from(error: Failure<Self>) -> Self {
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

#[cfg(all(feature = "usb", target_os = "linux"))]
impl From<rsl_usb::Error> for BusError {
    fn from(error: rsl_usb::Error) -> Self {
        match error {
            rsl_usb::Error::TimedOut => Self::Timeout,
            rsl_usb::Error::Disconnected => Self::Disconnected,
            rsl_usb::Error::Io(error) => Self::Io(error),
            other => Self::Other(Box::new(other)),
        }
    }
}

/// Register, SPI and I2C access to one uSDR board, plus time.
///
/// Implementations perform each call as one transaction and keep the order of calls.
pub trait Bus: Send {
    /// Reads consecutive 32-bit FPGA registers starting at `addr`.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError>;

    /// Writes consecutive 32-bit FPGA registers starting at `addr`.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError>;

    /// Shifts one 32-bit word through the SPI target and returns the word read back.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError>;

    /// Writes `write` to an I2C device, then fills `read`, both in wire order.
    ///
    /// Real transports accept at most [`I2C_MAX_WRITE`] and [`I2C_MAX_READ`] bytes.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] over those limits; any transport failure.
    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError>;

    /// Waits for `duration`. Board code never reads the clock itself, so a simulator can
    /// make this virtual.
    fn sleep(&mut self, duration: Duration);

    /// Sets up transport buffers for RX blocks of `block_bytes` (the transport's
    /// `stream_initialize`). `PCIe` uses 32 DMA buffers; USB queues eight transfers with
    /// additional trailer space. The driver has already configured the engine's registers.
    ///
    /// The default supports no streaming, for buses that only control the board.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        let _ = block_bytes;
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Waits up to `timeout` for the next filled RX block, lends it to `consume` with the
    /// gateware's two out-of-band words, then hands the buffer back to the DMA engine (the
    /// transport's `recv_dma_wait` and `recv_dma_release`).
    ///
    /// The out-of-band words, as libusdr's stream code reads them: the first holds the lost
    /// packet count in bits 23:0 and the completed-burst mask in bits 63:32; the second the
    /// DMA status in bits 31:0 for `PCIe`, or USB's TX statistics. The driver uses only
    /// the lost packet count.
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`] if no block arrives in time; [`BusError::Unsupported`] by
    /// default; any transport failure.
    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        let _ = (timeout, consume);
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Releases the RX DMA buffers (the transport's `stream_deinitialize`).
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        Err(BusError::Unsupported("RX streaming"))
    }
}
