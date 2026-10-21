//! What the `LSDr`'s I2C chips share: byte registers behind a pointer byte.

use std::iter;

/// A chip on an I2C bus, addressed with a register pointer as the first written byte.
pub(crate) trait I2cChip {
    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8>;
}

/// 256 byte-wide registers addressed by an 8-bit pointer, as both the TPS6594 and the
/// Si549 drivers address them (`tps6594_reg_rd`/`_wr`, `si549_reg_rd`/`_wr`: one pointer
/// byte, then one data byte).
///
/// Assumption: a multi-byte transfer auto-increments the pointer. libusdr only ever moves
/// one register per transfer on these chips, so no trace depends on it.
#[derive(Debug)]
pub(crate) struct ByteRegisters {
    /// Register contents, indexed by address.
    regs: [u8; 256],
    /// Addresses whose contents writes leave alone.
    read_only: &'static [u8],
}

impl ByteRegisters {
    /// All-zero registers, of which `read_only` ignore writes.
    pub(crate) fn new(read_only: &'static [u8]) -> Self {
        Self {
            regs: [0; 256],
            read_only,
        }
    }

    /// The same registers with `addr` holding `value`, for power-on contents.
    pub(crate) fn with(mut self, addr: u8, value: u8) -> Self {
        self.regs[usize::from(addr)] = value;
        self
    }

    /// A register's current contents.
    pub(crate) fn get(&self, addr: u8) -> u8 {
        self.regs[usize::from(addr)]
    }

    /// Clears the bits of `mask` in register `addr`, as a chip's self-clearing bits do.
    pub(crate) fn clear(&mut self, addr: u8, mask: u8) {
        self.regs[usize::from(addr)] &= !mask;
    }
}

impl I2cChip for ByteRegisters {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&pointer, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        let mut addresses = iter::successors(Some(pointer), |p| Some(p.wrapping_add(1)));
        for (&byte, addr) in data.iter().zip(addresses.by_ref()) {
            if !self.read_only.contains(&addr) {
                self.regs[usize::from(addr)] = byte;
            }
        }
        // The pointer has moved past the written bytes, so a read continues from there.
        addresses
            .take(read_len)
            .map(|addr| self.get(addr))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_then_a_read_of_the_same_register_returns_it() {
        let mut regs = ByteRegisters::new(&[]);
        regs.transfer(&[0x20, 0x31], 0);
        assert_eq!(regs.transfer(&[0x20], 1), vec![0x31]);
    }

    #[test]
    fn read_only_registers_keep_their_power_on_contents() {
        let mut regs = ByteRegisters::new(&[0]).with(0, 0x31);
        regs.transfer(&[0x00, 0x55], 0);
        assert_eq!(regs.get(0), 0x31);
    }

    #[test]
    fn a_transfer_without_a_pointer_reads_the_idle_bus() {
        let mut regs = ByteRegisters::new(&[]);
        assert_eq!(regs.transfer(&[], 2), vec![0xff, 0xff]);
    }
}
