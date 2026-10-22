//! TCA6424A 24-bit I2C GPIO expander: fe4ch4007200's four control expanders.
//!
//! # What it does
//!
//! Twenty-four pins in three 8-bit ports. Each port has an input, output,
//! polarity-inversion and configuration register; a configuration bit of 0 makes the pin
//! an output. Bit 7 of the register byte (`0x80 | reg`) makes the pointer advance after
//! each byte, so one transfer moves two ports.
//!
//! # Sources
//!
//! libusdr's `hw/tca6424a/tca6424a.c` and `tca6424a.h` (`enum tca6424a_regs`, the
//! transfers); no datasheet is vendored.

use hal::{Bus, I2cAddr};

use crate::error::{BusContext, Error};

/// Register addresses (`enum tca6424a_regs`), each the first of its three ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Port 0's output latch (`TCA6424_OUT0`).
    Output0 = 4,

    /// Port 0's direction; a 0 bit makes the pin an output (`TCA6424_CFG0`).
    Config0 = 12,
}

impl Reg {
    /// The register `port` ports past this one (0 to 2).
    const fn port(self, port: u8) -> u8 {
        self as u8 + port
    }
}

/// A TCA6424A on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tca6424a {
    /// Where the expander answers.
    dev: I2cAddr,
}

impl Tca6424a {
    /// The chip's name in errors.
    const NAME: &'static str = "TCA6424A";

    /// The register byte's auto-increment flag (`0x80 | reg`).
    const AUTO_INCREMENT: u8 = 0x80;

    /// The expander at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Writes the register `port` ports past `reg` (`tca6424a_reg8_set`).
    pub(crate) fn write8(
        self,
        bus: &mut dyn Bus,
        reg: Reg,
        port: u8,
        value: u8,
    ) -> Result<(), Error> {
        let reg = reg.port(port);
        bus.i2c(self.dev, &[reg, value], &mut [])
            .writing(Self::NAME, reg)
    }

    /// Writes `reg`'s first two ports, port 0 from the low byte (`tca6424a_reg16_set`).
    pub(crate) fn write16(self, bus: &mut dyn Bus, reg: Reg, value: u16) -> Result<(), Error> {
        let reg = reg.port(0);
        let [low, high] = value.to_le_bytes();
        bus.i2c(self.dev, &[Self::AUTO_INCREMENT | reg, low, high], &mut [])
            .writing(Self::NAME, reg)
    }

    /// Makes every pin an output: the first two configuration ports, then the third.
    pub(crate) fn make_outputs(self, bus: &mut dyn Bus) -> Result<(), Error> {
        self.write16(bus, Reg::Config0, 0)?;
        self.write8(bus, Reg::Config0, 2, 0)
    }

    /// Reads `reg`'s first two ports as libusdr sees them (`tca6424a_reg16_get`): the FPGA
    /// I2C core packs the bytes first-received-most-significant, which reaches libusdr's
    /// buffer little-endian, so port 0 lands in the low byte.
    pub(crate) fn read16(self, bus: &mut dyn Bus, reg: Reg) -> Result<u16, Error> {
        let reg = reg.port(0);
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[Self::AUTO_INCREMENT | reg], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(u16::from_le_bytes(raw))
    }
}
