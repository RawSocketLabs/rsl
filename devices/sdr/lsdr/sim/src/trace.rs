//! The ordered record of every operation a driver performed on a [`SimBoard`](crate::SimBoard).

use std::fmt;

use crate::I2cAddress;

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

/// One line per operation, the line format of the characterization golden traces:
/// `reg-wr ADDR VALUE`, `reg-rd ADDR VALUE`, `spi BUS OUT READ`,
/// `i2c BUS:ADDR w=HEX r=HEX` and `sleep US`. Numbers are fixed-width hex except the sleep.
impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegWrite { addr, value } => write!(f, "reg-wr {addr:#06x} {value:#010x}"),
            Self::RegRead { addr, value } => write!(f, "reg-rd {addr:#06x} {value:#010x}"),
            Self::Spi { bus, out, read } => write!(f, "spi {bus} {out:#010x} {read:#010x}"),
            Self::I2c { addr, write, read } => {
                write!(f, "i2c {addr} w=")?;
                write_hex(f, write)?;
                write!(f, " r=")?;
                write_hex(f, read)
            }
            Self::Sleep { us } => write!(f, "sleep {us}"),
        }
    }
}

/// Writes `bytes` as unseparated lowercase hex pairs, or `-` when there are none.
fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    if bytes.is_empty() {
        return f.write_str("-");
    }
    bytes.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_i2c_directions_render_as_a_dash_so_lines_stay_parseable() {
        let op = Op::I2c {
            addr: I2cAddress { bus: 0, addr: 0x48 },
            write: vec![0x20, 0x20],
            read: vec![],
        };
        assert_eq!(op.to_string(), "i2c 0:0x48 w=2020 r=-");
    }
}
