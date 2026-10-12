//! The DAC80501 that trims the XTRX reference oscillator (revision 5 boards).
//!
//! Access, from `xsdr_ctrl.c` (`_dac_x0501_set_reg`, `_dac_x0501_get_reg`): a register
//! pointer byte, then 16-bit data most significant byte first; a read writes the pointer
//! and reads two bytes.
//!
//! Assumption: `DEVID` (register 1) reads 0x0195. libusdr accepts 0x0194, 0x0195 and
//! 0x2195 alike (`_xsdr_init_revo`); the datasheet is not vendored, so which one a fitted
//! `DAC80501M` reports is unverified. Other registers reset to 0.

use std::collections::BTreeMap;
use std::iter;

use super::I2cChip;

/// `DEVID`, the device identification register.
const DEVID: u8 = 1;
/// What [`DEVID`] reads; see the module docs.
const DEVID_VALUE: u16 = 0x0195;

/// One DAC80501.
#[derive(Debug)]
pub(crate) struct Dac80501 {
    /// Registers written so far, by pointer.
    regs: BTreeMap<u8, u16>,
}

impl Dac80501 {
    /// A DAC fresh out of reset.
    pub(crate) fn new() -> Self {
        Self {
            regs: BTreeMap::from([(DEVID, DEVID_VALUE)]),
        }
    }
}

impl I2cChip for Dac80501 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let Some((&pointer, data)) = write.split_first() else {
            return vec![0xff; read_len];
        };
        if let [hi, lo] = *data {
            self.regs.insert(pointer, u16::from_be_bytes([hi, lo]));
        }
        let value = self.regs.get(&pointer).copied().unwrap_or(0);
        // libusdr never reads past the two data bytes; as a sim choice the bus idles high.
        value
            .to_be_bytes()
            .into_iter()
            .chain(iter::repeat(0xff))
            .take(read_len)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devid_reads_a_value_libusdr_accepts_msb_first() {
        let mut dac = Dac80501::new();
        let id = dac.transfer(&[DEVID], 2);
        assert!(
            [0x0194, 0x0195, 0x2195].contains(&u16::from_be_bytes([id[0], id[1]])),
            "_xsdr_init_revo rejects any other DEVID"
        );
    }

    #[test]
    fn a_register_write_is_read_back() {
        let mut dac = Dac80501::new();
        dac.transfer(&[8, 0xa8, 0xf5], 0);
        assert_eq!(dac.transfer(&[8], 2), vec![0xa8, 0xf5]);
    }
}
