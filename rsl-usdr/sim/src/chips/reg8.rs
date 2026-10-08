//! A plain 8-bit-address, 8-bit-data I2C register file (`LP8758` PMIC, `TPS6381x` boost).

use std::iter;

use super::I2cChip;

/// 256 byte-wide registers with an auto-incrementing pointer.
#[derive(Debug)]
pub(crate) struct Reg8File {
    /// Register contents, indexed by address.
    pub(crate) regs: [u8; 256],
}

impl Reg8File {
    /// A register file with the given non-zero reset values; all others reset to zero.
    pub(crate) fn with_resets(resets: &[(u8, u8)]) -> Self {
        let mut regs = [0; 256];
        for &(addr, value) in resets {
            regs[usize::from(addr)] = value;
        }
        Self { regs }
    }
}

impl I2cChip for Reg8File {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&pointer, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        let mut addresses =
            iter::successors(Some(pointer), |p| Some(p.wrapping_add(1))).map(usize::from);
        for (&byte, addr) in data.iter().zip(addresses.by_ref()) {
            self.regs[addr] = byte;
        }
        // The pointer auto-increments past written bytes, so a read continues from there.
        addresses
            .take(read_len)
            .map(|addr| self.regs[addr])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_after_write_continues_past_the_written_bytes() {
        let mut file = Reg8File::with_resets(&[(0x12, 0xab)]);
        assert_eq!(file.transfer(&[0x10, 1, 2], 1), vec![0xab]);
        assert_eq!(file.regs[0x10..0x12], [1, 2]);
    }
}
