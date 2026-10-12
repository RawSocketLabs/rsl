//! A 16-bit-address, 8-bit-data I2C register file (the LMK5C33216 clock synchronizer).

use super::I2cChip;

/// 65536 byte-wide registers behind a two-byte address, most significant byte first, with
/// an auto-incrementing address.
///
/// Modelling assumptions: libusdr's LMK drivers send the address high byte first
/// (`lmk_5c33216_reg_wr`: `{reg >> 8, reg, value}`) and read four ID bytes from address 0
/// in one transfer, which needs the address to advance per byte. Every register accepts
/// writes, including the read-only ID registers; libusdr's register load rewrites those
/// with the values they already hold.
#[derive(Debug)]
pub(crate) struct Reg16File {
    /// Register contents, indexed by address.
    regs: Vec<u8>,
}

impl Reg16File {
    /// A register file with the given non-zero reset values; all others reset to zero.
    pub(crate) fn with_resets(resets: &[(u16, u8)]) -> Self {
        let mut regs = vec![0; 1 << 16];
        for &(addr, value) in resets {
            regs[usize::from(addr)] = value;
        }
        Self { regs }
    }

    /// The register at `addr`.
    pub(crate) fn reg(&self, addr: u16) -> u8 {
        self.regs[usize::from(addr)]
    }
}

impl I2cChip for Reg16File {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let [high, low, data @ ..] = write else {
            return vec![0xff; read_len];
        };
        let mut addr = u16::from_be_bytes([*high, *low]);
        for &byte in data {
            self.regs[usize::from(addr)] = byte;
            addr = addr.wrapping_add(1);
        }
        (0..read_len)
            .map(|_| {
                let byte = self.regs[usize::from(addr)];
                addr = addr.wrapping_add(1);
                byte
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_advance_a_big_endian_address() {
        let mut file = Reg16File::with_resets(&[(0x0102, 0xaa), (0x0103, 0xbb)]);
        assert_eq!(file.transfer(&[0x01, 0x02], 2), vec![0xaa, 0xbb]);
        file.transfer(&[0x05, 0x00, 0x42], 0);
        assert_eq!(file.reg(0x0500), 0x42);
    }
}
