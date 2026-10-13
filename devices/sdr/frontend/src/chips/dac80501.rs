//! `DACx0501` single-channel voltage DAC: tunes the reference oscillator on the `PCIe` and
//! M.2-to-`PCIe` front-end boards.
//!
//! # What it does
//!
//! A 12-, 14- or 16-bit DAC (DAC60501, DAC70501, DAC80501) with an internal reference.
//! Its output steers the front end's voltage-controlled oscillator, so its code trims
//! the board's reference frequency. Registers are 16 bits wide behind an 8-bit pointer
//! and travel MSB first.
//!
//! # Sources
//!
//! Register map and fields: libusdr's `hw/dac80501/dac80501.yaml`. The accepted device IDs
//! are `dac80501_init`'s (`hw/dac80501/dac80501.c`).

use bnb::bitfield;

use crate::error::{BusContext, Error};
use rsl_lowlevel::{Bus, I2cAddr};

/// Register addresses (`dac80501.yaml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Device identification, read-only: resolution and power-on-reset level (`DEVID`).
    DeviceId = 0x01,

    /// Reference divider and output buffer gain; see [`Gain`] (`GAIN`).
    Gain = 0x04,

    /// The output code (`DAC`).
    Code = 0x08,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// Reference divider and output buffer gain. `GAIN`, 0x04.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gain {
    /// Divide the reference voltage by 2. `REF_DIV`, bit 8.
    #[bits(8..=8)]
    reference_halved: bool,

    /// Give the output buffer a gain of 2 rather than 1. `BUFF_GAIN`, bit 0.
    #[bits(0..=0)]
    buffer_gain_2: bool,
}

/// What the chip at the DAC's address identified as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Identity {
    /// A `DACx0501` libusdr knows; it has been configured.
    Known,

    /// Nothing that answers as a DAC: the ID read 0x0000 or 0xBEEF (libusdr's `-ENODEV`).
    Absent,

    /// Some other ID (libusdr's `-EIO`).
    Unknown,
}

/// A `DACx0501` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dac80501 {
    /// Where the DAC answers.
    dev: I2cAddr,
}

impl Dac80501 {
    /// The chip's name in errors.
    const NAME: &'static str = "DAC80501";

    /// The IDs `dac80501_init` accepts: the preproduction PDAC80501M (0x0194), then the
    /// production DAC80501M, DAC70501M, DAC60501M, DAC80501Z, DAC70501Z and DAC60501Z.
    const KNOWN_IDS: [u16; 7] = [0x0194, 0x0195, 0x1195, 0x2195, 0x0115, 0x1115, 0x2115];

    /// IDs libusdr takes for no DAC at all.
    const ABSENT_IDS: [u16; 2] = [0x0000, 0xbeef];

    /// The configuration libusdr applies (`DAC80501_CFG_REF_DIV_GAIN_MUL`): reference
    /// halved, buffer gain 2, so the output spans the full reference.
    const GAIN: Gain = Gain::new()
        .with_reference_halved(true)
        .with_buffer_gain_2(true);

    /// The DAC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Identifies the DAC and, if libusdr knows it, sets its gain (`dac80501_init`).
    pub(crate) fn init(self, bus: &mut dyn Bus) -> Result<Identity, Error> {
        let id = self.read(bus, Reg::DeviceId)?;
        if Self::ABSENT_IDS.contains(&id) {
            return Ok(Identity::Absent);
        }
        if !Self::KNOWN_IDS.contains(&id) {
            return Ok(Identity::Unknown);
        }
        self.write(bus, Reg::Gain, Self::GAIN.to_raw())?;
        Ok(Identity::Known)
    }

    /// Sets the output code (`dac80501_dac_set`).
    pub(crate) fn set_code(self, bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        self.write(bus, Reg::Code, code)
    }

    /// Reads a 16-bit register, MSB first.
    fn read(self, bus: &mut dyn Bus, reg: Reg) -> Result<u16, Error> {
        let reg = u8::from(reg);
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(u16::from_be_bytes(raw))
    }

    /// Writes a 16-bit register, MSB first.
    fn write(self, bus: &mut dyn Bus, reg: Reg, value: u16) -> Result<(), Error> {
        let reg = u8::from(reg);
        let [high, low] = value.to_be_bytes();
        bus.i2c(self.dev, &[reg, high, low], &mut [])
            .writing(Self::NAME, reg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libusdrs_gain_configuration_is_0x0101() {
        // `(uint16_t)MAKE_DAC80501_GAIN(1, 1)` in `dac80501_init`.
        assert_eq!(Dac80501::GAIN.to_raw(), 0x0101);
    }
}
