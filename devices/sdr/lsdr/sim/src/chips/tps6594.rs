//! The TPS6594 PMIC: buck/LDO set-points and enables (`hw/tps6594/tps6594.c`).
//!
//! Assumption: every register powers on as 0, including the identification bytes
//! `DEV_REV` (0x01), `NVM_CODE_1` (0x02) and `NVM_CODE_2` (0x03). The datasheet's values
//! are not vendored, and `tps6594_check` only logs what it reads, so no value changes
//! libusdr's path; the traces show the zeros.

use super::I2cChip;
use super::i2c::ByteRegisters;

/// One TPS6594.
#[derive(Debug)]
pub(crate) struct Tps6594 {
    /// Its registers.
    regs: ByteRegisters,
}

impl Tps6594 {
    /// A PMIC at power-on.
    pub(crate) fn new() -> Self {
        Self {
            regs: ByteRegisters::new(&[]),
        }
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u8) -> u8 {
        self.regs.get(addr)
    }
}

impl I2cChip for Tps6594 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        self.regs.transfer(write, read_len)
    }
}
