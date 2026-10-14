//! TCA6416 16-bit I2C GPIO expander: the `PCIe` front-end board's two control expanders.
//!
//! # What it does
//!
//! Sixteen pins in two 8-bit ports. Each port has an input, output, polarity-inversion and
//! configuration register; a configuration bit of 0 makes the pin an output. A transfer
//! starts at a register and continues with the other register of its pair, so one write
//! sets both ports' outputs.
//!
//! # Sources
//!
//! The register map is libusdr's `enum tca6416_regs` in `device/ext_pciefe/ext_pciefe.c`;
//! no register YAML is vendored for this chip, and its datasheet is not vendored here. The
//! output values themselves are the `PCIe` front end's (`pciefe.yaml`), owned by that board.

use crate::error::{BusContext, Error};
use hal::{Bus, I2cAddr};

/// Register addresses (`enum tca6416_regs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Port 0 pin levels, read-only (`IN_P0`).
    InputPort0 = 0,

    /// Port 1 pin levels, read-only (`IN_P1`).
    InputPort1 = 1,

    /// Port 0 output latch (`OUT_P0`).
    OutputPort0 = 2,

    /// Port 1 output latch (`OUT_P1`).
    OutputPort1 = 3,

    /// Port 0 direction; a 0 bit makes the pin an output (`CONFIG_P0`).
    ConfigPort0 = 6,

    /// Port 1 direction; a 0 bit makes the pin an output (`CONFIG_P1`).
    ConfigPort1 = 7,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// A TCA6416 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tca6416 {
    /// Where the expander answers.
    dev: I2cAddr,
}

impl Tca6416 {
    /// The chip's name in errors.
    const NAME: &'static str = "TCA6416";

    /// The expander at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Writes one register (`tca6416_reg_wr`).
    pub(crate) fn write(self, bus: &mut dyn Bus, reg: Reg, value: u8) -> Result<(), Error> {
        bus.i2c(self.dev, &[reg.into(), value], &mut [])
            .writing(Self::NAME, reg)
    }

    /// Writes `reg` and then the other register of its pair (`tca6416_reg_wr2`).
    pub(crate) fn write_pair(
        self,
        bus: &mut dyn Bus,
        reg: Reg,
        [first, second]: [u8; 2],
    ) -> Result<(), Error> {
        bus.i2c(self.dev, &[reg.into(), first, second], &mut [])
            .writing(Self::NAME, reg)
    }

    /// Reads four bytes from `reg` as libusdr sees them (`tca6416_reg_rd4`): the FPGA I2C
    /// core packs the bytes first-received-most-significant into one word, which reaches
    /// libusdr as a native `u32`.
    pub(crate) fn read_word(self, bus: &mut dyn Bus, reg: Reg) -> Result<u32, Error> {
        let mut raw = [0; 4];
        bus.i2c(self.dev, &[reg.into()], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(u32::from_be_bytes(raw))
    }
}

/// Register addresses as error context.
impl From<Reg> for u32 {
    fn from(reg: Reg) -> Self {
        u8::from(reg).into()
    }
}
