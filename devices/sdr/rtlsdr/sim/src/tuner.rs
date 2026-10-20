//! The R820T2's register file at its I2C port, modelled from the Rafael *R820T2 Register
//! Description* independently of `rtlsdr`'s code (ledger rows in `rtlsdr`'s
//! `PROVENANCE.md`). Power-on contents other than R0 are not documented (`R820-GAP-08`):
//! they start at zero, and nothing models the PLL, filters or detectors behind them.

use std::fmt;

/// Registers in the file. prov: R820-DS-02
const REGISTER_COUNT: usize = 32;

/// The first writable register. prov: R820-DS-06
const FIRST_WRITABLE: usize = 0x05;

/// R0's fixed content. prov: R820-DS-05
const CHIP_ID: u8 = 0x96;

/// An I2C transaction the chip would not acknowledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunerRefusal {
    /// A write with no register address.
    Empty,

    /// A write reaching a read-only register or past the last one.
    NotWritable {
        /// The first register.
        first: u8,

        /// Data bytes after the address.
        count: usize,
    },

    /// A read past the last register.
    TooLong(usize),
}

impl fmt::Display for TunerRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("I2C write without a register address"),
            Self::NotWritable { first, count } => {
                write!(
                    f,
                    "I2C write of {count} bytes from {first:#04x} not writable"
                )
            }
            Self::TooLong(len) => write!(f, "I2C read of {len} bytes exceeds the registers"),
        }
    }
}

impl std::error::Error for TunerRefusal {}

/// The tuner's register file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct R820t2 {
    /// The registers, by address.
    registers: [u8; REGISTER_COUNT],
}

impl Default for R820t2 {
    fn default() -> Self {
        let mut registers = [0; REGISTER_COUNT];
        registers[0] = CHIP_ID;
        Self { registers }
    }
}

impl R820t2 {
    /// A page write: `bytes[0]` names the first register, the rest fill it and those after.
    /// prov: R820-DS-03
    ///
    /// # Errors
    ///
    /// [`TunerRefusal`] for an empty write or one touching R0-R4 or past R31; nothing is
    /// written then.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), TunerRefusal> {
        let (&first, data) = bytes.split_first().ok_or(TunerRefusal::Empty)?;
        let start = usize::from(first);
        if start < FIRST_WRITABLE || start + data.len() > REGISTER_COUNT {
            return Err(TunerRefusal::NotWritable {
                first,
                count: data.len(),
            });
        }
        self.registers[start..start + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// A read of `len` bytes, from R0 on, each sent least significant bit first, so as an
    /// MSB-first master receives it. prov: R820-DS-04
    ///
    /// # Errors
    ///
    /// [`TunerRefusal::TooLong`] past R31.
    pub fn read(&self, len: usize) -> Result<Vec<u8>, TunerRefusal> {
        let registers = self
            .registers
            .get(..len)
            .ok_or(TunerRefusal::TooLong(len))?;
        Ok(registers.iter().map(|byte| byte.reverse_bits()).collect())
    }

    /// The registers, by address.
    #[must_use]
    pub fn registers(&self) -> &[u8; REGISTER_COUNT] {
        &self.registers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r0_reads_back_as_the_chip_id_sent_lsb_first() {
        let tuner = R820t2::default();
        assert_eq!(tuner.read(1).unwrap(), [CHIP_ID.reverse_bits()]);
        assert_eq!(tuner.read(33), Err(TunerRefusal::TooLong(33)));
    }

    #[test]
    fn a_page_write_fills_consecutive_registers_and_refuses_read_only_ones() {
        let mut tuner = R820t2::default();
        tuner.write(&[0x1E, 0xAA, 0x55]).unwrap();
        assert_eq!(tuner.registers()[0x1E..], [0xAA, 0x55]);
        assert!(tuner.write(&[0x04, 0x00]).is_err(), "R4 is read-only");
        assert!(tuner.write(&[0x1F, 0, 0]).is_err(), "R32 does not exist");
        assert_eq!(tuner.write(&[]), Err(TunerRefusal::Empty));
        assert_eq!(
            tuner.registers()[0],
            CHIP_ID,
            "refused writes change nothing"
        );
    }
}
