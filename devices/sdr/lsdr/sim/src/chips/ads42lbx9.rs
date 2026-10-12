//! The `ADS42LBx9` dual-channel ADC (`hw/ads42lbx9/ads42lbx9.c`).
//!
//! Frame, as libusdr builds it: bit 15 set for a read, register address in bits 13:8,
//! data in bits 7:0; a read returns the register in the low byte (`ads42lbx9_rd` keeps
//! only that byte). Assumption: the chip sees the 32-bit SPI word's low 16 bits, and
//! returns 0 above the data byte and for writes. The gateware's SPI framing is not in the
//! source; libusdr passes the bus number alone as the `ls_op` address.

/// `REG_HF0` (0x0D): `ads42lbx9_reset_and_check` reads it to find the chip.
const REG_HF0: usize = 0x0d;
/// `REG_HF0`'s power-on contents. Evidence: `ads42lbx9_reset_and_check` reports the chip
/// present only when `(HF0 & 0xFE) == 0x6C`; the datasheet is not vendored, so the sim
/// takes 0x6C. Every other register powers on as 0 (assumption).
const HF0_POWER_ON: u8 = 0x6c;
/// Bit 15 of the frame: read.
const READ: u32 = 0x8000;

/// One `ADS42LBx9`.
#[derive(Debug)]
pub(crate) struct Ads42lbx9 {
    /// The 64 registers the 6-bit address reaches.
    regs: [u8; 64],
}

impl Ads42lbx9 {
    /// An ADC at power-on.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 64];
        regs[REG_HF0] = HF0_POWER_ON;
        Self { regs }
    }

    /// Shifts one SPI word through the chip and returns what it drives back.
    pub(crate) fn transact(&mut self, out: u32) -> u32 {
        let reg = usize::from(((out >> 8) & 0x3f) as u8);
        if out & READ != 0 {
            return u32::from(self.regs[reg]);
        }
        let [.., data] = out.to_be_bytes();
        self.regs[reg] = data;
        0
    }

    /// A register's current contents.
    pub(crate) fn reg(&self, addr: u8) -> u8 {
        self.regs[usize::from(addr & 0x3f)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_adc_passes_libusdrs_presence_check() {
        let mut adc = Ads42lbx9::new();
        let hf0 = adc.transact(READ | 0x0d << 8);
        assert_eq!(hf0 & 0xfe, 0x6c);
    }

    #[test]
    fn a_written_register_reads_back() {
        let mut adc = Ads42lbx9::new();
        adc.transact(0x08 << 8 | 0xe0);
        assert_eq!(adc.transact(READ | 0x08 << 8), 0xe0);
        assert_eq!(adc.reg(0x08), 0xe0);
    }
}
