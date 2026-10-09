//! The LP8758 driver: revision check, configuration and per-channel control.

use super::buck::{Buck, BuckControl, BuckVoltage};
use super::config::Config;
use super::id::Revision;
use super::reg::Reg;
use crate::chips::register::I2cChip;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl I2cChip for Lp8758 {
    const NAME: &'static str = "LP8758";

    fn addr(&self) -> I2cAddr {
        self.addr
    }
}

impl Lp8758 {
    /// The revision libusdr requires: OTP revision 0xE0, device revision 0x01.
    const REVISION: Revision = Revision::new(0xe0, 0x01);

    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The PMIC where the uSDR wires it: FPGA I2C bus 0, the LP8758-E0's address 0x60
    /// (SNVSAC6B; `I2C_DEV_PMIC_FPGA` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x60))
    }

    /// Checks the revision is the one libusdr accepts.
    pub(crate) fn check_revision(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut regs = self.on(bus);

        // libusdr reads the device revision first.
        let device = regs.read_raw(Reg::DeviceRevision)?;
        let otp = regs.read_raw(Reg::OtpRevision)?;

        match Revision::new(otp, device) {
            Self::REVISION => Ok(()),
            found => Err(Error::ChipId {
                chip: Self::NAME,
                expected: Self::REVISION.to_raw().into(),
                found: found.to_raw().into(),
            }),
        }
    }

    /// Writes the configuration register.
    pub(crate) fn configure(self, bus: &mut dyn Bus, config: Config) -> Result<(), Error> {
        self.on(bus).write(config)
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

    /// Writes a channel's control register.
    pub(crate) fn control(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        control: BuckControl,
    ) -> Result<(), Error> {
        self.on(bus).write_to(buck, control)
    }
}
