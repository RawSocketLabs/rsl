//! The register file shared by the TI SPI parts (LMX2820, LMX1204, LMX1214).
//!
//! Word format, from each chip's generated header (`gen/def_lmx2820.h` and siblings:
//! `MAKE_*_REG_WR(a, v) = a << 16 | v`, `MAKE_*_REG_RD(a) = 0x800000 | a << 16`) and the
//! 24-bit buses `pe_sync.c` documents: bit 23 selects a read, bits 22:16 carry the 7-bit
//! address and bits 15:0 the data. Bits 31:24 are not shifted out; the file ignores them.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Undocumented addresses reset to 0 and are plain read/write storage.
//! - Read-only bits (YAML `mode: R`) ignore writes. The YAMLs mark reserved bits `R` too, so
//!   those keep their reset value as well.
//! - No reset side effects: setting a chip's `RESET` bit is stored like any other bit and
//!   does not reload the reset values.
//! - A write reads back 0. libusdr discards what MISO carries during a write
//!   (`common_spi_post` passes no read buffer), so the value is unconstrained.

/// Bit 23 of an SPI word: a read (`MAKE_*_REG_RD`).
const READ: u8 = 0x80;

/// One register's power-on state: address, reset value, read-only mask.
pub(crate) type Reset = (u8, u16, u16);

/// A chip's 128 16-bit registers.
#[derive(Debug)]
pub(crate) struct SpiRegisters {
    /// Register contents, indexed by address.
    regs: [u16; 128],
    /// Bits writes cannot change, indexed by address.
    read_only: [u16; 128],
}

impl SpiRegisters {
    /// A chip fresh out of power-on reset.
    pub(crate) fn new(reset: &[Reset]) -> Self {
        let mut chip = Self {
            regs: [0; 128],
            read_only: [0; 128],
        };
        for &(addr, value, read_only) in reset {
            chip.regs[usize::from(addr)] = value;
            chip.read_only[usize::from(addr)] = read_only;
        }
        chip
    }

    /// The same chip with the read-only bits `mask` of register `addr` reading `value`: a
    /// status the chip reports instead of its reset value.
    ///
    /// # Panics
    ///
    /// When `mask` covers writable bits, which would let the driver overwrite the status.
    pub(crate) fn with_status(mut self, addr: u8, mask: u16, value: u16) -> Self {
        let addr = usize::from(addr);
        assert_eq!(
            self.read_only[addr] & mask,
            mask,
            "invariant: a status occupies read-only bits"
        );
        self.regs[addr] = self.regs[addr] & !mask | value & mask;
        self
    }

    /// Shifts one SPI word through the chip and returns the word read back: the register's
    /// value in bits 15:0 for a read, 0 for a write.
    pub(crate) fn transact(&mut self, word: u32) -> u32 {
        let [_, command, data_hi, data_lo] = word.to_be_bytes();
        let addr = command & !READ;
        if command & READ != 0 {
            return u32::from(self.reg(addr));
        }
        let addr = usize::from(addr);
        let kept = self.regs[addr] & self.read_only[addr];
        self.regs[addr] = kept | u16::from_be_bytes([data_hi, data_lo]) & !self.read_only[addr];
        0
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u8) -> u16 {
        self.regs[usize::from(addr & !READ)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R1 writable with reset 0x1234, R2 half read-only.
    const RESET: [Reset; 2] = [(0x01, 0x1234, 0x0000), (0x02, 0xab00, 0xff00)];

    #[test]
    fn a_read_returns_the_reset_value_in_the_low_half() {
        let mut chip = SpiRegisters::new(&RESET);
        assert_eq!(chip.transact(0x0081_0000), 0x1234);
    }

    #[test]
    fn a_write_keeps_read_only_bits() {
        let mut chip = SpiRegisters::new(&RESET);
        assert_eq!(chip.transact(0x0002_ffff), 0);
        assert_eq!(chip.reg(0x02), 0xabff);
    }

    #[test]
    fn a_status_replaces_read_only_bits_only() {
        let chip = SpiRegisters::new(&RESET).with_status(0x02, 0x0300, 0x0200);
        assert_eq!(chip.reg(0x02), 0xaa00);
    }
}
