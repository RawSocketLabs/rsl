//! TI LMK05318 network synchronizer clock: the clock tree of the `SimpleSync` and XMASS
//! front ends. Only its identification is ported.
//!
//! # What it does
//!
//! A DPLL plus two APLLs that discipline a reference and fan it out on eight outputs.
//! Registers are bytes behind a 16-bit address, sent most significant byte first.
//!
//! # Sources
//!
//! libusdr's `hw/lmk05318/lmk05318.c`: `lmk05318_create` reads four bytes from address 0
//! and requires `10 0B 35 42`. Its register YAML does not document addresses 0 to 3, so
//! the ID stays four raw bytes. The rest of `lmk05318_create` (XO, DPLL, APLL and output
//! solving) is not ported.

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum Reg {
    /// The first identification byte; libusdr reads four from here. Undocumented in the
    /// vendored YAML.
    Id = 0x0000,
}

impl From<Reg> for u16 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// An LMK05318 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lmk05318 {
    /// Where the chip answers.
    dev: I2cAddr,
}

impl Lmk05318 {
    /// The chip's name in errors.
    pub(crate) const NAME: &'static str = "LMK05318";

    /// The identification bytes from address 0, in wire order.
    pub(crate) const ID: [u8; 4] = [0x10, 0x0b, 0x35, 0x42];

    /// The chip at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Reads the four identification bytes from address 0, in wire order
    /// (`lmk05318_reg_get_u32(.., 0, ..)`).
    pub(crate) fn read_id(self, bus: &mut dyn Bus) -> Result<[u8; 4], Error> {
        let reg = u16::from(Reg::Id);
        let mut raw = [0; 4];
        bus.i2c(self.dev, &reg.to_be_bytes(), &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(raw)
    }
}
