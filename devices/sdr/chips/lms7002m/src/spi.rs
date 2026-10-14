//! The SPI word: one 32-bit exchange per register access.
//!
//! `gen/def_lms7002m.h` (`MAKE_LMS7002M_REG_WR`/`_RD`) and the YAML's `wr_mask: 0x80000000`:
//! bit 31 selects a write, bits 30:16 carry the address and bits 15:0 the data. A read
//! returns the register in bits 15:0 of the reply (`lms7002m_spi_rd` keeps those).

use bnb::{bitfield, u15};
use hal::{FpgaAccess, SpiAddr};

use super::reg::Reg;

/// One SPI transaction word.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Word {
    /// Set for a write, clear for a read.
    #[bits(31..=31)]
    write: bool,

    /// The register address.
    #[bits(16..=30)]
    addr: u15,

    /// The value written; zero for reads.
    #[bits(0..=15)]
    data: u16,
}

/// The exchange that writes `value` to `reg` on the chip at `target`.
pub(crate) fn write(target: SpiAddr, (reg, value): (Reg, u16)) -> FpgaAccess {
    access(target, reg, true, value)
}

/// The exchange that reads `reg` on the chip at `target`. Public as `spi_read`, for board
/// code that reads a register outside this crate's operations, such as a board's own
/// version gate before [`Create`](crate::Create).
#[must_use]
pub fn read(target: SpiAddr, reg: Reg) -> FpgaAccess {
    access(target, reg, false, 0)
}

/// The register value a read's reply word carries (bits 15:0).
pub(crate) const fn value(reply: u32) -> u16 {
    let [lo, hi, ..] = reply.to_le_bytes();
    u16::from_le_bytes([lo, hi])
}

/// One exchange; the access names the register for error reports.
fn access(target: SpiAddr, reg: Reg, write: bool, data: u16) -> FpgaAccess {
    let addr = reg as u16;
    let word = Word::new()
        .with_write(write)
        .with_addr(u15::new(addr))
        .with_data(data);
    FpgaAccess::Spi {
        target,
        word: word.to_raw(),
        reg: addr.into(),
        write,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_the_generated_macros() {
        // MAKE_LMS7002M_REG_WR(XBUF_0x0085, 0x0199) and MAKE_LMS7002M_REG_RD(LML_0x002F).
        let target = SpiAddr(0);
        assert!(matches!(
            write(target, (Reg::Xbuf, 0x0199)),
            FpgaAccess::Spi {
                word: 0x8085_0199,
                reg: 0x0085,
                write: true,
                ..
            }
        ));
        assert!(matches!(
            read(target, Reg::Version),
            FpgaAccess::Spi {
                word: 0x002f_0000,
                reg: 0x002f,
                write: false,
                ..
            }
        ));
        assert_eq!(
            value(0xdead_3840),
            0x3840,
            "only bits 15:0 are the register"
        );
    }
}
