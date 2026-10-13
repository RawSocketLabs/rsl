//! TCA9555-compatible 16-bit I2C GPIO expander: the XMASS front end's control expander
//! (an `XRA1201` by libusdr's comment, driven with libusdr's TCA9555 helpers).
//!
//! # What it does
//!
//! Sixteen pins in two 8-bit ports, each with input, output, polarity and configuration
//! registers; a configuration bit of 0 makes the pin an output. The driver reads and writes
//! both ports of a register pair in one transfer, port 0 first.
//!
//! # Sources
//!
//! Register addresses: libusdr's `enum tca9555_regs` (`hw/tca9555/tca9555.h`); 16-bit
//! access as `tca9555_reg16_get`/`tca9555_reg16_set` perform it.

use crate::error::{BusContext, Error};
use rsl_lowlevel::{Bus, I2cAddr};

/// Register addresses: the port-0 register of each pair (`enum tca9555_regs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Output latches; a 1 bit drives the pin high where it is an output (`TCA9555_OUT0`).
    Output0 = 2,

    /// Port direction; a 0 bit makes the pin an output, 1 an input (`TCA9555_CFG0`).
    Config0 = 6,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// A TCA9555 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tca9555 {
    /// Where the expander answers.
    dev: I2cAddr,
}

impl Tca9555 {
    /// The chip's name in errors.
    const NAME: &'static str = "TCA9555";

    /// The expander at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Reads a register pair, port 1 in the high byte (`tca9555_reg16_get`).
    pub(crate) fn read16(self, bus: &mut dyn Bus, reg: Reg) -> Result<u16, Error> {
        let reg = u8::from(reg);
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(u16::from_le_bytes(raw))
    }

    /// Writes a register pair, port 0 from the low byte (`tca9555_reg16_set`).
    pub(crate) fn write16(self, bus: &mut dyn Bus, reg: Reg, value: u16) -> Result<(), Error> {
        let reg = u8::from(reg);
        let [low, high] = value.to_le_bytes();
        bus.i2c(self.dev, &[reg, low, high], &mut [])
            .writing(Self::NAME, reg)
    }
}
