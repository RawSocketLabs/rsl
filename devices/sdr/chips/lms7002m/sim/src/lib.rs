//! The LMS7002M RF transceiver's SPI register file, shared by the simulated boards that
//! carry the chip (`rsl-xsdr-sim`, `rsl-limesdr-sim`).
//!
//! Word format, from libusdr's register YAML (`hw/lms7002m/lms7002m.yaml`: `wr_mask:
//! 0x80000000`, 16-bit address and data) and `gen/def_lms7002m.h` (`MAKE_LMS7002M_REG_WR`,
//! `MAKE_LMS7002M_REG_RD`): bit 31 selects a write, bits 30:16 carry the address and bits
//! 15:0 the data.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Reset values: the vendored YAML and header document fields but no reset values, so
//!   every register reads 0 until written, except the read-only version register below.
//!   No libusdr path the drivers port reads a register it has not written except that one,
//!   the VCO comparators and the processors' built-in self-test status. The self-test
//!   status (`RXTSP` `0x040E`, `TXTSP` `0x0209`, read by `lms7002m_xxtsp_bst_isdone`) is
//!   never written, so it reads 0: bit 0 clear, which libusdr takes for "done".
//! - VCO comparators (`CGEN_0x008C`, `SXX_0x0123`): with a reference clock given
//!   ([`Lms7002m::with_reference`]), their comparator bits follow the cap code and divider
//!   the driver programmed; the model and its assumptions are in `vco.rs`. Without one
//!   they read as stored, like any register.
//! - Channel banking: the chip duplicates some registers per channel, selected by
//!   `LML_0x0020` MAC bits; which ones is not in the vendored source, so the file is flat
//!   and MAC writes are stored like any other register. libusdr selects the bank before
//!   each comparator read and rewrites every register the read depends on first, so a
//!   flat file reads the same there.
//! - Soft resets (`LML_0x0020` reset bits) do not clear the file: with no reset values to
//!   restore, clearing would invent them. The reset pin does ([`Lms7002m::reset`]).
#![forbid(unsafe_code)]

mod vco;

use std::collections::BTreeMap;

/// Bit 31 of an SPI word: a write (`wr_mask` in the YAML).
const WRITE: u32 = 1 << 31;
/// The address bits of an SPI word's upper half; bit 15 there is [`WRITE`].
const ADDR_MASK: u16 = 0x7fff;
/// `LML_0x002F`: chip version (`VER`, bits 15:11), revision (`REV`, 10:6) and metal mask
/// (`MASK`, 5:0).
const VERSION: u16 = 0x002f;
/// `VER` 7 and `REV` 1, the only values `lms7002m_create` accepts (`hw/lms7002m/lms7002m.c`:
/// `-ENODEV` otherwise).
const VER_REV: u16 = 7 << 11 | 1 << 6;
/// The `MASK` field's width: bits 5:0.
const MASK_FIELD: u8 = 0x3f;

/// The LMS7002M's 15-bit-addressed, 16-bit-wide register file.
#[derive(Debug)]
pub struct Lms7002m {
    /// Registers written since power-on or the last reset, by address.
    regs: BTreeMap<u16, u16>,
    /// What [`VERSION`] reads.
    version: u16,
    /// The reference clock in hertz, which the VCO comparators need; `None` reads them as
    /// stored.
    reference: Option<u64>,
}

impl Lms7002m {
    /// A chip fresh out of power-on reset whose version register reports `VER` 7, `REV` 1
    /// and metal-mask revision `mask`. libusdr's generic driver ignores the mask; a board
    /// driver may not (the `u3_limesdr` family's `limesdr_init` accepts only 1).
    ///
    /// # Panics
    ///
    /// When `mask` does not fit the 6-bit `MASK` field.
    #[must_use]
    pub fn new(mask: u8) -> Self {
        assert!(mask <= MASK_FIELD, "MASK is a 6-bit field, got {mask:#x}");
        Self {
            regs: BTreeMap::new(),
            version: VER_REV | u16::from(mask),
            reference: None,
        }
    }

    /// The same chip clocked from a `hertz` reference, so its VCO comparators answer the
    /// cap codes a tuning search writes (see the crate docs).
    #[must_use]
    pub fn with_reference(mut self, hertz: u32) -> Self {
        self.reference = Some(hertz.into());
        self
    }

    /// Shifts one SPI word through the chip and returns the word read back: the register's
    /// value in bits 15:0 for a read, 0 for a write. libusdr keeps only bits 15:0 of a read
    /// (`lms7002m_spi_rd`), so what MISO carries during the address phase is unconstrained;
    /// the sim returns 0 there.
    pub fn transact(&mut self, word: u32) -> u32 {
        let [addr_hi, addr_lo, data_hi, data_lo] = word.to_be_bytes();
        let addr = u16::from_be_bytes([addr_hi, addr_lo]);
        if word & WRITE != 0 {
            self.write(addr, u16::from_be_bytes([data_hi, data_lo]));
            return 0;
        }
        u32::from(self.reg(addr))
    }

    /// Writes a register. Bit 15 of `addr`, the SPI write flag, is not part of the address.
    /// The version register is read-only.
    pub fn write(&mut self, addr: u16, value: u16) {
        let addr = addr & ADDR_MASK;
        if addr != VERSION {
            self.regs.insert(addr, value);
        }
    }

    /// A register as the chip reports it. Bit 15 of `addr` is ignored, as in
    /// [`Lms7002m::write`].
    #[must_use]
    pub fn reg(&self, addr: u16) -> u16 {
        let addr = addr & ADDR_MASK;
        let stored = |addr| self.regs.get(&addr).copied().unwrap_or(0);
        match (addr, self.reference) {
            (VERSION, _) => self.version,
            (vco::CGEN_COMPARATORS, Some(reference)) => {
                vco::merge(stored(addr), vco::cgen(stored, reference))
            }
            (vco::SXX_COMPARATORS, Some(reference)) => {
                vco::merge(stored(addr), vco::sxx(stored, reference))
            }
            _ => stored(addr),
        }
    }

    /// Pulses the chip's reset pin (`RESET`): every register returns to its power-on state,
    /// which this model represents as unwritten (see the crate docs on reset values).
    pub fn reset(&mut self) {
        self.regs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_reads_the_only_chip_libusdr_accepts() {
        let mut chip = Lms7002m::new(0);
        let ver = chip.transact(u32::from(VERSION) << 16);
        assert_eq!((ver >> 11) & 0x1f, 7, "VER must be 7");
        assert_eq!((ver >> 6) & 0x1f, 1, "REV must be 1");
    }

    #[test]
    fn the_metal_mask_is_the_version_registers_low_bits() {
        assert_eq!(
            Lms7002m::new(1).reg(VERSION),
            0x3841,
            "LimeSDR's required word"
        );
        assert_eq!(Lms7002m::new(0).reg(VERSION), 0x3840);
    }

    #[test]
    #[should_panic(expected = "6-bit field")]
    fn a_mask_wider_than_its_field_is_refused() {
        let _ = Lms7002m::new(0x40);
    }

    #[test]
    fn a_write_is_read_back_and_the_write_flag_is_not_part_of_the_address() {
        let mut chip = Lms7002m::new(0);
        assert_eq!(chip.transact(WRITE | 0x0092 << 16 | 0xbeef), 0);
        assert_eq!(chip.transact(0x0092 << 16), 0xbeef);
        chip.write(0x8093, 0x1234);
        assert_eq!(chip.reg(0x0093), 0x1234);
    }

    #[test]
    fn the_version_register_ignores_writes() {
        let mut chip = Lms7002m::new(0);
        chip.transact(WRITE | u32::from(VERSION) << 16);
        assert_eq!(chip.reg(VERSION), 0x3840);
    }

    #[test]
    fn comparators_answer_only_with_a_reference_and_keep_their_other_bits() {
        // CGEN at 90 * 26 MHz; cap 0 is far too fast for it.
        let program = |chip: &mut Lms7002m| {
            chip.write(0x0088, 89 << 4);
            chip.write(0x008b, 0);
            chip.write(0x008c, 0x0f0f);
        };
        let mut plain = Lms7002m::new(0);
        program(&mut plain);
        assert_eq!(plain.transact(0x008c << 16), 0x0f0f, "read as stored");

        let mut clocked = Lms7002m::new(0).with_reference(26_000_000);
        program(&mut clocked);
        clocked.write(0x008b, 255 << 1);
        assert_eq!(
            clocked.transact(0x008c << 16),
            0x3f0f,
            "the slowest code reads high (both comparators) over the stored bits"
        );
    }

    #[test]
    fn reset_is_power_on_except_for_the_version() {
        let mut chip = Lms7002m::new(1);
        chip.write(0x0020, 0xffff);
        chip.reset();
        assert_eq!(chip.reg(0x0020), 0, "a reset forgets every write");
        assert_eq!(
            chip.reg(VERSION),
            0x3841,
            "the version is silicon, not state"
        );
    }
}
