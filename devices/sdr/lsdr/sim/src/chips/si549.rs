//! The Si549 programmable oscillator, the ADC sample clock (`hw/si549/si549.{c,yaml}`).
//!
//! Register 0 (`DEVICE` `TYPE`) reads 0x31: the vendored YAML calls it a "read only
//! value of 49 (dec)", and `si549_set_freq` refuses any other. Every other register
//! powers on as 0 (assumption: the YAML documents no reset values). The `CFG` bits the
//! YAML calls self-clearing (register 7: `RESET`, bit 7, and `MS_ICAL2`, bit 3) clear as
//! soon as they are written: the sim completes the reset or calibration at once, and
//! models neither (a sim choice; libusdr never reads `CFG` back).

use super::I2cChip;
use super::i2c::ByteRegisters;

/// `DEVICE` (register 0): the device type.
const DEVICE: u8 = 0;
/// `DEVICE` `TYPE` of an Si549.
const DEVICE_TYPE: u8 = 0x31;
/// `CFG` (register 7).
const CFG: u8 = 7;
/// `CFG`'s self-clearing `RESET` (bit 7) and `MS_ICAL2` (bit 3).
const CFG_SELF_CLEARING: u8 = 0x88;

/// One Si549.
#[derive(Debug)]
pub(crate) struct Si549 {
    /// Its registers.
    regs: ByteRegisters,
}

impl Si549 {
    /// An oscillator at power-on.
    pub(crate) fn new() -> Self {
        Self {
            regs: ByteRegisters::new(&[DEVICE]).with(DEVICE, DEVICE_TYPE),
        }
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u8) -> u8 {
        self.regs.get(addr)
    }
}

impl I2cChip for Si549 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let read = self.regs.transfer(write, read_len);
        self.regs.clear(CFG, CFG_SELF_CLEARING);
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calibration_start_clears_itself() {
        let mut osc = Si549::new();
        osc.transfer(&[CFG, 0x0a], 0);
        assert_eq!(osc.reg(CFG), 0x02);
    }

    #[test]
    fn the_device_type_is_the_one_si549_set_freq_accepts_and_cannot_be_overwritten() {
        let mut osc = Si549::new();
        osc.transfer(&[DEVICE, 0], 0);
        assert_eq!(osc.transfer(&[DEVICE], 1), vec![0x31]);
    }
}
