//! Raw register access to the board's chips, for debugging: libusdr's
//! `/debug/hw/lms6002d/0/reg`, `/debug/hw/si5332/0/reg` and `/debug/hw/tps6381x/0/reg`
//! (`device/m2_lm6_1/m2_lm6_1.c`, `dev_m2_lm6_1_debug_*_reg_set` and `_get`).
//!
//! libusdr packs each access into one number, a write flag, the register and the byte
//! (`value & 0xffff` as the LMS6002D's SPI word; bit 23 or bit 15 marking an I2C write),
//! and keeps the byte read for its getter. Here the register and byte are arguments and a
//! read returns the byte. The accesses bypass the drivers' cached state, as libusdr's do.

use bnb::u7;

use super::Board;
use crate::chips::lms6002d::Lms6002d;
use crate::error::{BusContext, Error};
use crate::lowlevel::I2cAddr;
use crate::portable::{si5332, tps6381x};

/// A chip whose registers [`Device::chip_register`](crate::Device::chip_register) and
/// [`Device::set_chip_register`](crate::Device::set_chip_register) reach raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DebugChip {
    /// The LMS6002D transceiver, over SPI: registers 0x00 to 0x7f.
    Lms6002d,

    /// The Si5332 clock generator, over I2C.
    Si5332,

    /// The `TPS6381x` buck-boost converter, over I2C.
    Tps6381x,
}

impl DebugChip {
    /// The chip's name in errors.
    const fn name(self) -> &'static str {
        match self {
            Self::Lms6002d => Lms6002d::NAME,
            Self::Si5332 => si5332::NAME,
            Self::Tps6381x => tps6381x::NAME,
        }
    }

    /// Where an I2C chip answers; `None` for the LMS6002D, on SPI.
    const fn i2c(self) -> Option<I2cAddr> {
        match self {
            Self::Lms6002d => None,
            Self::Si5332 => Some(Board::CLOCK.dev()),
            Self::Tps6381x => Some(Board::BOOST.dev()),
        }
    }

    /// The LMS6002D's 7-bit address for `reg`.
    fn lms_reg(self, reg: u8) -> Result<u7, Error> {
        (reg <= 0x7f)
            .then(|| u7::new(reg))
            .ok_or(Error::NoSuchRegister {
                chip: self.name(),
                reg,
            })
    }
}

impl Board {
    /// Reads `chip`'s register `reg` (libusdr's debug register set with the write flag
    /// clear, then its get): an LMS6002D read word, or an I2C write of the register then a
    /// one-byte read.
    pub(crate) fn chip_register(&mut self, chip: DebugChip, reg: u8) -> Result<u8, Error> {
        let Some(dev) = chip.i2c() else {
            return self
                .lms
                .read_register_raw(self.bus.as_mut(), chip.lms_reg(reg)?);
        };
        let mut byte = [0];
        self.bus
            .i2c(dev, &[reg], &mut byte)
            .reading(chip.name(), reg)?;
        Ok(byte[0])
    }

    /// Writes `value` to `chip`'s register `reg` (libusdr's debug register set with the
    /// write flag): an LMS6002D write word, or an I2C write of the register and the byte.
    ///
    /// Intentional divergence for the `TPS6381x`: libusdr's write flag is bit 15, which it
    /// leaves in the register byte it sends, so it writes register `reg | 0x80`; this
    /// writes `reg`.
    pub(crate) fn set_chip_register(
        &mut self,
        chip: DebugChip,
        reg: u8,
        value: u8,
    ) -> Result<(), Error> {
        let Some(dev) = chip.i2c() else {
            return self
                .lms
                .write_register_raw(self.bus.as_mut(), chip.lms_reg(reg)?, value);
        };
        self.bus
            .i2c(dev, &[reg, value], &mut [])
            .writing(chip.name(), reg)
    }
}
