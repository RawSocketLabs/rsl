//! The LMS7002M RF transceiver's SPI register file.
//!
//! Word format, from libusdr's register YAML (`hw/lms7002m/lms7002m.yaml`: `wr_mask:
//! 0x80000000`, 16-bit address and data) and `gen/def_lms7002m.h` (`MAKE_LMS7002M_REG_WR`,
//! `MAKE_LMS7002M_REG_RD`): bit 31 selects a write, bits 30:16 carry the address and bits
//! 15:0 the data.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Reset values: the vendored YAML and header document fields but no reset values, so
//!   every register reads 0 until written, except the read-only version register below.
//!   Init/power-up never reads a register it has not written except that one.
//! - Channel banking: the chip duplicates some registers per channel, selected by
//!   `LML_0x0020` MAC bits; which ones is not in the vendored source, so the file is flat
//!   and MAC writes are stored like any other register.
//! - Soft resets (`LML_0x0020` reset bits) do not clear the file: with no reset values to
//!   restore, clearing would invent them.

use std::collections::BTreeMap;

/// Bit 31 of an SPI word: a write (`wr_mask` in the YAML).
const WRITE: u32 = 1 << 31;
/// `LML_0x002F`: chip version (`VER`, bits 15:11), revision (`REV`, 10:6) and metal mask
/// (`MASK`, 5:0).
const VERSION: u16 = 0x002f;
/// What [`VERSION`] reads: `VER` 7 and `REV` 1, the only values `lms7002m_create` accepts
/// (`hw/lms7002m/lms7002m.c`: `-ENODEV` otherwise). `MASK` is not checked by the C; 0 is a
/// sim choice.
const VERSION_VALUE: u16 = 7 << 11 | 1 << 6;

/// The LMS7002M's 15-bit-addressed, 16-bit-wide register file.
#[derive(Debug)]
pub(crate) struct Lms7002m {
    /// Registers written so far, by address.
    regs: BTreeMap<u16, u16>,
}

impl Lms7002m {
    /// A chip fresh out of power-on reset.
    pub(crate) fn new() -> Self {
        Self {
            regs: BTreeMap::new(),
        }
    }

    /// Shifts one SPI word through the chip and returns the word read back: the register's
    /// value in bits 15:0 for a read, 0 for a write. libusdr keeps only bits 15:0 of a read
    /// (`lms7002m_spi_rd`), so what MISO carries during the address phase is unconstrained;
    /// the sim returns 0 there.
    pub(crate) fn transact(&mut self, word: u32) -> u32 {
        let [addr_hi, addr_lo, data_hi, data_lo] = word.to_be_bytes();
        let addr = u16::from_be_bytes([addr_hi & 0x7f, addr_lo]);
        if word & WRITE != 0 {
            // The version register is read-only.
            if addr != VERSION {
                self.regs
                    .insert(addr, u16::from_be_bytes([data_hi, data_lo]));
            }
            return 0;
        }
        u32::from(self.reg(addr))
    }

    /// A register as the chip reports it.
    pub(crate) fn reg(&self, addr: u16) -> u16 {
        if addr == VERSION {
            return VERSION_VALUE;
        }
        self.regs.get(&addr).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_reads_the_only_chip_libusdr_accepts() {
        let mut chip = Lms7002m::new();
        let ver = chip.transact(u32::from(VERSION) << 16);
        assert_eq!((ver >> 11) & 0x1f, 7, "VER must be 7");
        assert_eq!((ver >> 6) & 0x1f, 1, "REV must be 1");
    }

    #[test]
    fn a_write_is_read_back_and_the_write_flag_is_not_part_of_the_address() {
        let mut chip = Lms7002m::new();
        assert_eq!(chip.transact(WRITE | 0x0092 << 16 | 0xbeef), 0);
        assert_eq!(chip.transact(0x0092 << 16), 0xbeef);
    }

    #[test]
    fn the_version_register_ignores_writes() {
        let mut chip = Lms7002m::new();
        chip.transact(WRITE | u32::from(VERSION) << 16);
        assert_eq!(chip.reg(VERSION), VERSION_VALUE);
    }
}
