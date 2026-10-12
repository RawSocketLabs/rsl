//! The LP8758 driver: where the uSDR wires it and the per-channel voltage the board moves
//! after power-up. Power-up itself runs the portable `PowerRails` machine.

use crate::chips::register::I2cChip;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};
use crate::portable::lp8758::{self, Buck, BuckVoltage};

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl I2cChip for Lp8758 {
    const NAME: &'static str = lp8758::NAME;

    fn addr(&self) -> I2cAddr {
        self.addr
    }
}

impl Lp8758 {
    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The PMIC where the uSDR wires it: FPGA I2C bus 0, the LP8758-E0's address 0x60
    /// (SNVSAC6B; `I2C_DEV_PMIC_FPGA` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x60))
    }

    /// Where the chip answers, for a sequence machine's plan.
    pub(crate) const fn dev(self) -> I2cAddr {
        self.addr
    }

    /// Sets a channel's output voltage.
    pub(crate) fn set_voltage(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        voltage: BuckVoltage,
    ) -> Result<(), Error> {
        self.on(bus).write_to(buck, voltage)
    }
}
