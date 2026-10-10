//! The ordered record of every operation a driver performed on a [`SimBoard`](crate::SimBoard).

/// One low-level operation, as seen by the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A 32-bit FPGA register write.
    RegWrite {
        /// Register address.
        addr: u32,
        /// Value written.
        value: u32,
    },
    /// A 32-bit FPGA register read.
    RegRead {
        /// Register address.
        addr: u32,
        /// Value returned.
        value: u32,
    },
    /// A 32-bit SPI transaction.
    Spi {
        /// SPI bus number.
        bus: u32,
        /// Word shifted out.
        out: u32,
        /// Word read back.
        read: u32,
    },
    /// An I2C write-then-read transfer.
    I2c {
        /// Device address.
        addr: I2cAddress,
        /// Bytes written, register pointer first.
        write: Vec<u8>,
        /// Bytes read, in wire order.
        read: Vec<u8>,
    },
    /// The driver slept.
    Sleep {
        /// Virtual microseconds slept.
        us: u64,
    },
}

use crate::I2cAddress;
