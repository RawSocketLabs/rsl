//! The hardware seam: what a transport (USB, `PCIe`, or a simulator) must provide.
//!
//! **Unstable before 1.0.** This mirrors libusdr's `lowlevel_ops.ls_op` so custom buses,
//! including the board simulator, can drive a [`Device`](crate::Device). Streaming methods
//! join the trait when streaming is ported.

use std::error::Error as StdError;
use std::io;
use std::time::Duration;

/// Most bytes one I2C transfer can write: the FPGA I2C core's command word holds three.
pub const I2C_MAX_WRITE: usize = 3;
/// Most bytes one I2C transfer can read: the FPGA I2C core returns one 32-bit word.
pub const I2C_MAX_READ: usize = 4;

/// An I2C device: the FPGA I2C bus it sits on and its 7-bit address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct I2cAddr {
    /// FPGA I2C bus number.
    pub bus: u8,
    /// 7-bit device address.
    pub addr: u8,
}

/// An SPI target as libusdr encodes it: bus number in bits 7:0, core configuration in
/// bits 31:16 (unused on the uSDR).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpiAddr(pub u32);

impl SpiAddr {
    /// The SPI bus number.
    #[must_use]
    pub fn bus(self) -> u8 {
        self.0.to_le_bytes()[0]
    }
}

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
}
