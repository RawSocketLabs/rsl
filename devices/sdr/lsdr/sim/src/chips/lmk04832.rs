//! The LMK04832 clock generator, fitted on dual-ADC boards (`hw/lmk04832/lmk04832.{c,yaml}`,
//! generated `def_lmk04832.h`).
//!
//! Frame, as `lmk04832.c` builds it: 24 bits, bit 23 set for a read
//! (`MAKE_LMK04832_RDREG`), address in bits 22:8, data in bits 7:0; a read returns the
//! register in the low byte (`lmk04832_read16` keeps `& 0xff`). The generated header's
//! `MAKE_LMK04832_REG_RD` and the YAML's `rd_mask` put the read bit at 31 instead; the
//! open path does not use them. Assumption, as for the ADC: the chip sees the word's low
//! 24 bits and drives 0 above the data byte.
//!
//! Readback leaves the chip on its RESET pin, so it is driven only while `RESET_CFG`
//! selects `RESET_MUX_SPI_READBACK` (6) with `RESET_TYPE` an output. Evidence:
//! `lmk04832_reset_and_test` writes both (`RESET_TYPE_OUTPUT_PUSH_PULL`, 3) before reading
//! the IDs, and the YAML says `RESET_MUX` "only applies if `RESET_TYPE` is set to an output
//! mode"; push-pull is the only output type it documents, so the sim accepts that one.
//! Otherwise the line floats and, as for absent devices, reads all ones (a sim choice). Writing `RESET` bit 7 returns every register to power-on, and the
//! bit clears itself (YAML: "Reset (automatically cleared)").

use std::collections::BTreeMap;

/// Bit 23 of the frame: read.
const READ: u32 = 0x80_0000;
/// `RESET` (R0); bit 7 resets the chip.
const RESET: u16 = 0x000;
/// `RESET` bit 7.
const RESET_BIT: u8 = 0x80;
/// `RESET_CFG` (R330): `RESET_MUX` in bits 5:3, `RESET_TYPE` in bits 2:0.
const RESET_CFG: u16 = 0x14a;
/// `RESET_MUX` = `RESET_MUX_SPI_READBACK`.
const RESET_MUX_SPI_READBACK: u8 = 6;
/// `RESET_TYPE` = `RESET_TYPE_OUTPUT_PUSH_PULL`.
const RESET_TYPE_OUTPUT_PUSH_PULL: u8 = 3;

/// The identification registers and the values the sim gives them. libusdr accepts the
/// chip when `PROD_LOW << 8 | PROD_HI` is 0xD163 and `VNDR_LOW << 8 | VNDR_HI` is 0x0451
/// (`lmk04832_read16` puts the second address in the high byte). The sim chooses bytes
/// that satisfy the C; the datasheet, which may order them the other way, is not vendored.
/// `LMKDEV_TYPE` (R3) and `MASKREV` (R6) are not checked and read 0. All read-only.
const ID: [(u16, u8); 6] = [
    (0x003, 0x00), // LMKDEV_TYPE
    (0x004, 0x63), // PROD_HI
    (0x005, 0xd1), // PROD_LOW
    (0x006, 0x00), // MASKREV
    (0x00c, 0x51), // VNDR_HI
    (0x00d, 0x04), // VNDR_LOW
];

/// One LMK04832.
#[derive(Debug)]
pub(crate) struct Lmk04832 {
    /// Registers that differ from 0, by 15-bit address. Assumption: all but the IDs power
    /// on as 0 (the YAML documents no reset values).
    regs: BTreeMap<u16, u8>,
}

impl Lmk04832 {
    /// A clock generator at power-on.
    pub(crate) fn new() -> Self {
        Self {
            regs: BTreeMap::from(ID),
        }
    }

    /// Shifts one SPI word through the chip and returns what its readback pin drives.
    pub(crate) fn transact(&mut self, out: u32) -> u32 {
        let addr = ((out >> 8) & 0x7fff) as u16;
        let [.., data] = out.to_be_bytes();
        let read = out & READ != 0;
        let response = if !self.readback_routed() {
            u32::MAX
        } else if read {
            u32::from(self.reg(addr))
        } else {
            0
        };
        if !read {
            self.write(addr, data);
        }
        response
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u16) -> u8 {
        self.regs.get(&addr).copied().unwrap_or(0)
    }

    /// Whether `RESET_CFG` routes register readback to the RESET pin and drives it.
    fn readback_routed(&self) -> bool {
        let cfg = self.reg(RESET_CFG);
        (cfg >> 3) & 0x7 == RESET_MUX_SPI_READBACK && cfg & 0x7 == RESET_TYPE_OUTPUT_PUSH_PULL
    }

    /// Applies a register write.
    fn write(&mut self, addr: u16, data: u8) {
        if ID.iter().any(|&(id, _)| id == addr) {
            return;
        }
        if addr == RESET && data & RESET_BIT != 0 {
            *self = Self::new();
            self.regs.insert(RESET, data & !RESET_BIT);
            return;
        }
        self.regs.insert(addr, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read frame for `addr`.
    fn read(addr: u16) -> u32 {
        READ | u32::from(addr) << 8
    }

    /// A write frame.
    fn write(addr: u16, data: u8) -> u32 {
        u32::from(addr) << 8 | u32::from(data)
    }

    /// `RESET_CFG` as `lmk04832_reset_and_test` writes it.
    const ROUTED: u8 = RESET_MUX_SPI_READBACK << 3 | RESET_TYPE_OUTPUT_PUSH_PULL;

    #[test]
    fn readback_floats_until_the_reset_pin_is_routed_to_it() {
        let mut lmk = Lmk04832::new();
        assert_eq!(lmk.transact(read(0x004)), u32::MAX);
        lmk.transact(write(RESET_CFG, ROUTED));
        assert_eq!(lmk.transact(read(0x004)), 0x63);
    }

    #[test]
    fn selecting_readback_without_an_output_pin_type_still_floats() {
        let mut lmk = Lmk04832::new();
        lmk.transact(write(RESET_CFG, RESET_MUX_SPI_READBACK << 3));
        assert_eq!(lmk.transact(read(0x004)), u32::MAX);
    }

    #[test]
    fn the_ids_assemble_to_what_libusdr_accepts() {
        let mut lmk = Lmk04832::new();
        lmk.transact(write(RESET_CFG, ROUTED));
        let pair =
            |lmk: &mut Lmk04832, addr| lmk.transact(read(addr + 1)) << 8 | lmk.transact(read(addr));
        assert_eq!(pair(&mut lmk, 0x004), 0xd163);
        assert_eq!(pair(&mut lmk, 0x00c), 0x0451);
    }

    #[test]
    fn a_reset_restores_power_on_and_clears_itself() {
        let mut lmk = Lmk04832::new();
        lmk.transact(write(RESET_CFG, ROUTED));
        lmk.transact(write(0x100, 0x12));
        lmk.transact(write(RESET, RESET_BIT | 0x10));
        assert_eq!(lmk.reg(0x100), 0);
        assert_eq!(lmk.reg(RESET_CFG), 0, "readback routing is lost too");
        assert_eq!(lmk.reg(RESET), 0x10);
    }

    #[test]
    fn identification_registers_ignore_writes() {
        let mut lmk = Lmk04832::new();
        lmk.transact(write(0x004, 0));
        assert_eq!(lmk.reg(0x004), 0x63);
    }
}
