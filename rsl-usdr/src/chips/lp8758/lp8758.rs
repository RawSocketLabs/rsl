//! The LP8758 driver: revision check, configuration and per-channel control.

use super::buck::{Buck, BuckControl, BuckVoltage};
use super::config::Config;
use super::reg::Reg;
use crate::chips::register::I2cRegisters;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Lp8758 {
    /// The revision libusdr requires: OTP revision 0xE0, device revision 0x01.
    const REVISION: u16 = 0xe001;

    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "LP8758 read", "LP8758 write"),
        }
    }

    /// The PMIC where the uSDR wires it: FPGA I2C bus 0, the LP8758-E0's address 0x60
    /// (SNVSAC6B; `I2C_DEV_PMIC_FPGA` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x60))
    }

    /// Checks the revision, `OTP_REV << 8 | DEV_REV` as libusdr composes it, is the one
    /// libusdr accepts.
    pub(crate) fn check_revision(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut regs = self.regs.on(bus);
        let device = regs.read_raw(Reg::DeviceRevision)?;
        let otp = regs.read_raw(Reg::OtpRevision)?;
        let found = u16::from_be_bytes([otp, device]);
        if found != Self::REVISION {
            return Err(Error::ChipId {
                chip: "LP8758",
                expected: Self::REVISION.into(),
                found: found.into(),
            });
        }
        Ok(())
    }

    /// Writes the configuration register.
    pub(crate) fn configure(self, bus: &mut dyn Bus, config: Config) -> Result<(), Error> {
        self.regs.on(bus).write(config)
    }

    /// Sets a channel's output voltage.
    pub(crate) fn set_voltage(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        voltage: BuckVoltage,
    ) -> Result<(), Error> {
        self.regs.on(bus).write_to(buck, voltage)
    }

    /// Writes a channel's control register.
    pub(crate) fn control(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        control: BuckControl,
    ) -> Result<(), Error> {
        self.regs.on(bus).write_to(buck, control)
    }
}
