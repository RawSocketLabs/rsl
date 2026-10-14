//! The SPI word of the TI clock chips on `PE_SYNC` (LMX2820, LMX1204, LMX1214): one 24-bit
//! exchange per register access.
//!
//! Each chip's generated header (`gen/def_lmx2820.h` and siblings) defines
//! `MAKE_*_REG_WR(a, v) = a << 16 | v` and `MAKE_*_REG_RD(a) = 0x800000 | a << 16`: bit 23
//! selects a read, bits 22:16 carry the 7-bit address and bits 15:0 the data. A read returns
//! the register in bits 15:0 of the reply (`common_spi_get` keeps those).

use bnb::{bitfield, u7};
use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::outcome::outcome;

/// A 16-bit register at one fixed address of one of these chips: its value type names
/// the address. `hal`'s `Register` traits are 8-bit.
pub(crate) trait Register: Copy + Into<u16> {
    /// The chip's register-address enum.
    type Map: Into<u8>;
    /// Where this register lives.
    const ADDR: Self::Map;

    /// This value as an `(address, value)` write.
    fn entry(self) -> (u8, u16) {
        (Self::ADDR.into(), self.into())
    }
}

/// One SPI transaction word.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Word {
    /// Set for a read, clear for a write.
    #[bits(23..=23)]
    read: bool,

    /// The register address.
    #[bits(16..=22)]
    addr: u7,

    /// The value written; zero for reads.
    #[bits(0..=15)]
    data: u16,
}

/// The exchange that writes an `(address, value)` entry to the chip at `target`.
pub(crate) const fn write(target: SpiAddr, (reg, value): (u8, u16)) -> SequenceAction {
    access(target, reg, false, value)
}

/// The exchange that reads register `reg` of the chip at `target`.
pub(crate) const fn read(target: SpiAddr, reg: u8) -> SequenceAction {
    access(target, reg, true, 0)
}

/// A read-modify-write of one register: read it, then write it back with `set` bits set
/// and `clear` bits cleared (`lmx1214_reset_main_divider` and its siblings).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Modify {
    /// The chip, for failure reports.
    chip: &'static str,
    /// Where it answers.
    target: SpiAddr,
    /// The register.
    reg: u8,
    /// Bits to set.
    set: u16,
    /// Bits to clear.
    clear: u16,
    /// The value read, once read.
    read: Option<u16>,
    /// Finished or failed.
    done: bool,
}

impl Modify {
    /// Sets `set` and clears `clear` in register `reg` of `chip` at `target`.
    pub(crate) const fn new(
        chip: &'static str,
        target: SpiAddr,
        reg: u8,
        set: u16,
        clear: u16,
    ) -> Self {
        Self {
            chip,
            target,
            reg,
            set,
            clear,
            read: None,
            done: false,
        }
    }
}

impl Sequence for Modify {
    fn next(&self) -> Option<SequenceAction> {
        if self.done {
            return None;
        }
        Some(match self.read {
            None => read(self.target, self.reg),
            Some(value) => write(self.target, (self.reg, (value | self.set) & !self.clear)),
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = outcome(action, result, self.chip).inspect_err(|_| self.done = true)?;
        match self.read {
            None => self.read = Some(value(word)),
            Some(_) => self.done = true,
        }
        Ok(())
    }
}

/// The register value a read's reply word carries (bits 15:0).
pub(crate) const fn value(reply: u32) -> u16 {
    let [lo, hi, ..] = reply.to_le_bytes();
    u16::from_le_bytes([lo, hi])
}

/// One exchange; the access names the register for error reports.
const fn access(target: SpiAddr, reg: u8, read: bool, data: u16) -> SequenceAction {
    let word = Word::new()
        .with_read(read)
        .with_addr(u7::from_raw(reg))
        .with_data(data);
    SequenceAction::Fpga(FpgaAccess::Spi {
        target,
        word: word.to_raw(),
        reg: reg as u32,
        write: !read,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_the_generated_macros() {
        // MAKE_LMX2820_R19(0x109, TEMPSENSE_EN_ENABLED, 0) and MAKE_LMX2820_REG_RD(R74).
        let target = SpiAddr(2);
        assert_eq!(
            write(target, (0x13, 0x2138)),
            SequenceAction::Fpga(FpgaAccess::Spi {
                target,
                word: 0x0013_2138,
                reg: 0x13,
                write: true,
            })
        );
        assert_eq!(
            read(target, 0x4a),
            SequenceAction::Fpga(FpgaAccess::Spi {
                target,
                word: 0x00ca_0000,
                reg: 0x4a,
                write: false,
            })
        );
        assert_eq!(
            value(0xdead_8000),
            0x8000,
            "only bits 15:0 are the register"
        );
    }
}
