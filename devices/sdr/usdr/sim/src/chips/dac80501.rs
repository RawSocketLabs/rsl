//! `DACx0501` voltage DAC: 8-bit pointer, 16-bit registers sent MSB first.

use super::I2cChip;

/// Device identification register, read-only (`DEVID`).
const DEVICE_ID: usize = 0x01;
/// The output code register (`DAC`).
const CODE: usize = 0x08;

/// A `DACx0501`.
///
/// Modelling assumptions: registers are 16 bits behind an 8-bit pointer and travel MSB
/// first in both directions, as libusdr's `dac80501.c` writes `{reg, msb, lsb}` and compares
/// a two-byte read against IDs such as 0x0195 (the FPGA I2C core packs the first byte most
/// significant). `DEVID` ignores writes. Other registers reset to zero; libusdr's YAML gives
/// no reset values and only its debug path reads them back.
#[derive(Debug)]
pub(crate) struct Dac80501 {
    /// Register contents, indexed by pointer.
    regs: [u16; 16],
}

impl Dac80501 {
    /// A DAC identifying as `id`.
    pub(crate) fn new(id: u16) -> Self {
        let mut regs = [0; 16];
        regs[DEVICE_ID] = id;
        Self { regs }
    }

    /// The output code.
    pub(crate) fn code(&self) -> u16 {
        self.regs[CODE]
    }
}

impl I2cChip for Dac80501 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&pointer, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        let index = usize::from(pointer & 0x0f);
        match *data {
            [hi, lo] if index != DEVICE_ID => self.regs[index] = u16::from_be_bytes([hi, lo]),
            _ => {}
        }
        self.regs[index]
            .to_be_bytes()
            .into_iter()
            .cycle()
            .take(read_len)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_reads_msb_first_and_ignores_writes() {
        let mut dac = Dac80501::new(0x0195);
        dac.transfer(&[0x01, 0, 0], 0);
        assert_eq!(dac.transfer(&[0x01], 2), vec![0x01, 0x95]);
        dac.transfer(&[0x08, 0x12, 0x34], 0);
        assert_eq!(dac.code(), 0x1234);
    }
}
