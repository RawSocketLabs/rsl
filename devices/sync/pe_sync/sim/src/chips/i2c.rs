//! The register file shared by the I2C parts (LMK1D1208I, LP87524): byte-wide
//! registers behind a register pointer of one or two bytes, sent MSB first, that
//! auto-increments past each byte written or read.
//!
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Undocumented addresses reset to 0 and are plain read/write storage.
//! - Read-only bits ignore writes.
//! - A transfer without a whole pointer reads the idle bus (`0xff`); libusdr always sends
//!   one.

use std::collections::BTreeMap;

/// One register's power-on state: address, reset value, read-only mask.
pub(crate) type Reset = (u16, u8, u8);

/// A chip's byte-wide registers.
#[derive(Debug)]
pub(crate) struct I2cRegisters {
    /// Register pointer width in bytes.
    pointer_len: usize,
    /// Register contents and read-only masks, by address; absent registers read 0.
    regs: BTreeMap<u16, (u8, u8)>,
}

impl I2cRegisters {
    /// A chip fresh out of power-on reset, addressed by a `pointer_len`-byte pointer.
    pub(crate) fn new(pointer_len: usize, reset: &[Reset]) -> Self {
        let regs = reset
            .iter()
            .map(|&(addr, value, read_only)| (addr, (value, read_only)))
            .collect();
        Self { pointer_len, regs }
    }

    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    pub(crate) fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((pointer, data)) = write.split_at_checked(self.pointer_len) else {
            return vec![0xff; read_len];
        };
        let start = pointer
            .iter()
            .fold(0u16, |addr, &byte| addr << 8 | u16::from(byte));
        let mut addr = start;
        for &byte in data {
            let (value, read_only) = self.regs.entry(addr).or_default();
            *value = *value & *read_only | byte & !*read_only;
            addr = addr.wrapping_add(1);
        }
        (0..read_len)
            .map(|_| {
                let value = self.reg(addr);
                addr = addr.wrapping_add(1);
                value
            })
            .collect()
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u16) -> u8 {
        self.regs.get(&addr).map_or(0, |&(value, _)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_byte_read_auto_increments_from_a_16_bit_pointer() {
        let mut chip = I2cRegisters::new(2, &[(0x0100, 0x11, 0), (0x0101, 0x22, 0)]);
        assert_eq!(chip.transfer(&[0x01, 0x00], 3), vec![0x11, 0x22, 0x00]);
    }

    #[test]
    fn a_write_keeps_read_only_bits() {
        let mut chip = I2cRegisters::new(1, &[(0x05, 0x20, 0xf0)]);
        chip.transfer(&[0x05, 0xff], 0);
        assert_eq!(chip.reg(0x05), 0x2f);
    }

    #[test]
    fn a_short_pointer_reads_the_idle_bus() {
        let mut chip = I2cRegisters::new(2, &[]);
        assert_eq!(chip.transfer(&[0x00], 1), vec![0xff]);
    }
}
