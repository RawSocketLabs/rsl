//! The LP8758 buck-converter PMIC: 256 byte-wide registers behind an auto-incrementing
//! pointer (`hw/lp8758/lp8758.c` addresses them with an 8-bit pointer byte).
//!
//! Assumption: registers other than the two revision bytes reset to 0, including the
//! power-good status (`BUCK_0_1_STAT` 0x1c, `BUCK_2_3_STAT` 0x1d). That cannot change a
//! trace: `lp8758_check_pg(.., 0xf, ..)`, the only call the init paths make, reports good
//! whenever the status-derived mask is a subset of 0xf, which every status value is, so
//! the C's power-good wait always ends on its first poll.

use std::iter;

use super::I2cChip;

/// `DEV_REV` (0x00) and `OTP_REV` (0x01): libusdr reads them as `OTP_REV << 8 | DEV_REV`.
const DEV_REV: usize = 0x00;
/// See [`DEV_REV`].
const OTP_REV: usize = 0x01;

/// One LP8758.
#[derive(Debug)]
pub(crate) struct Lp8758 {
    /// Register contents, indexed by address.
    regs: [u8; 256],
}

impl Lp8758 {
    /// A PMIC whose revision word (`OTP_REV << 8 | DEV_REV`) reads `revision`.
    pub(crate) fn with_revision(revision: u16) -> Self {
        let mut regs = [0; 256];
        [regs[OTP_REV], regs[DEV_REV]] = revision.to_be_bytes();
        Self { regs }
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u8) -> u8 {
        self.regs[usize::from(addr)]
    }
}

impl I2cChip for Lp8758 {
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
    fn the_revision_word_splits_low_byte_into_dev_rev() {
        let mut pmic = Lp8758::with_revision(0xe001);
        assert_eq!(pmic.transfer(&[0x00], 1), vec![0x01]);
        assert_eq!(pmic.transfer(&[0x01], 1), vec![0xe0]);
    }

    #[test]
    fn a_register_write_is_kept() {
        let mut pmic = Lp8758::with_revision(0xe001);
        pmic.transfer(&[0x0c, 0x9d], 0);
        assert_eq!(pmic.reg(0x0c), 0x9d);
    }
}
