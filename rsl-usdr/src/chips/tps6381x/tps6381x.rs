//! The `TPS6381x` driver: identification and start-up.

use super::control::Control;
use super::id::DeviceId;
use super::voltage::{OutputVoltage, Vsel};
use crate::chips::register::I2cRegisters;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Tps6381x {
    /// The ID libusdr requires: Texas Instruments, silicon revision B0.
    const DEVICE_ID: u8 = 0x04;

    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "TPS6381x ID read", "TPS6381x write"),
        }
    }

    /// The converter where the uSDR wires it: FPGA I2C bus 0, address 0x75, the part's fixed
    /// address (`I2C_DEV_DCDCBOOST` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x75))
    }

    /// Checks the device ID, then runs the converter at `voltage` whichever level VSEL is
    /// at (`tps6381x_init`).
    pub(crate) fn init(
        self,
        bus: &mut dyn Bus,
        forced_pwm: bool,
        voltage: OutputVoltage,
    ) -> Result<(), Error> {
        let mut regs = self.regs.on(bus);
        let id: DeviceId = regs.read()?;
        let ti_revision_b0 =
            id.manufacturer().value() == 0 && id.major().value() == 1 && id.minor().value() == 0;
        if !ti_revision_b0 {
            return Err(Error::ChipId {
                chip: "TPS6381x",
                expected: Self::DEVICE_ID.into(),
                found: id.to_raw().into(),
            });
        }
        let control = Control::new()
            .with_enabled(true)
            .with_forced_pwm(forced_pwm)
            .with_ramp_pwm(forced_pwm);
        regs.write(control)?;
        regs.write_to(Vsel::Low, voltage)?;
        regs.write_to(Vsel::High, voltage)
    }
}
