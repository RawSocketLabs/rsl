//! The LMS6002D SPI protocol: one 16-bit word per register access.
//!
//! The FPGA's SPI engine sends the word and returns the chip's reply; for a read, the
//! register's byte comes back in the low eight bits. The 7-bit address reaches 128 byte
//! registers, grouped into blocks by address range.

use bnb::{bitfield, u7};

use super::super::address::SpiAddr;
use super::super::sequence::FpgaAccess;

/// An LMS6002D block's register-address enum; only these address an [`SpiWord`].
pub(crate) trait BlockReg: Into<u8> {}

/// Any of the 128 register addresses, named by the caller rather than a block: the raw
/// debug access libusdr's `/debug/hw/lms6002d/0/reg` offers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AnyReg(pub(crate) u7);

impl From<AnyReg> for u8 {
    fn from(reg: AnyReg) -> Self {
        reg.0.value()
    }
}

impl BlockReg for AnyReg {}

/// One SPI transaction word.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpiWord {
    /// Set for a write, clear for a read.
    #[bits(15..=15)]
    write: bool,

    /// Register address.
    #[bits(8..=14)]
    addr: u7,

    /// The byte written; zero for reads.
    #[bits(0..=7)]
    data: u8,
}

/// The exchange reading `reg` on the chip at `target`.
pub(crate) fn read(target: SpiAddr, reg: impl BlockReg) -> FpgaAccess {
    let reg = reg.into();
    let word = SpiWord::new().with_addr(u7::new(reg));
    FpgaAccess::Spi {
        target,
        word: word.to_raw().into(),
        reg: reg.into(),
        write: false,
    }
}

/// The exchange writing `value` to `reg` on the chip at `target`.
pub(crate) fn write(target: SpiAddr, reg: impl BlockReg, value: u8) -> FpgaAccess {
    let reg = reg.into();
    let word = SpiWord::new()
        .with_write(true)
        .with_addr(u7::new(reg))
        .with_data(value);
    FpgaAccess::Spi {
        target,
        word: word.to_raw().into(),
        reg: reg.into(),
        write: true,
    }
}

/// The register byte a read's reply carries, in its low eight bits.
pub(crate) const fn value(reply: u32) -> u8 {
    reply.to_le_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_carry_the_address_and_byte_libusdr_shifts() {
        let reg = AnyReg(u7::new(0x09));
        assert!(matches!(
            read(SpiAddr(0), reg),
            FpgaAccess::Spi {
                word: 0x0900,
                reg: 9,
                write: false,
                ..
            }
        ));
        assert!(matches!(
            write(SpiAddr(0), reg, 0xc0),
            FpgaAccess::Spi {
                word: 0x89c0,
                reg: 9,
                write: true,
                ..
            }
        ));
        assert_eq!(value(0x1234_56ab), 0xab);
    }
}
