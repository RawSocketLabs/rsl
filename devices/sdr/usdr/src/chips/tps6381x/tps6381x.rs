//! The `TPS6381x` driver: identification and start-up.

use bnb::u2;

use super::control::Control;
use super::id::DeviceId;
use super::voltage::{OutputVoltage, Vsel};
use crate::chips::register::I2cChip;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl I2cChip for Tps6381x {
    const NAME: &'static str = "TPS6381x";

    fn addr(&self) -> I2cAddr {
        self.addr
    }
}

impl Tps6381x {
    /// The ID libusdr requires: Texas Instruments, silicon revision B0 (0x04).
    const DEVICE_ID: DeviceId = DeviceId::new().with_major(u2::new(1));

    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
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
        let mut regs = self.on(bus);
        let id: DeviceId = regs.read()?;
        match id {
            Self::DEVICE_ID => Ok(()),
            found => Err(Error::ChipId {
                chip: Self::NAME,
                expected: Self::DEVICE_ID.to_raw().into(),
                found: found.to_raw().into(),
            }),
        }?;
        let control = Control::new()
            .with_enabled(true)
            .with_forced_pwm(forced_pwm)
            .with_ramp_pwm(forced_pwm);
        regs.write(control)?;
        regs.write_to(Vsel::Low, voltage)?;
        regs.write_to(Vsel::High, voltage)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::error::Access;
    use crate::lowlevel::{BusError, SpiAddr};

    /// An I2C bus whose writes fail; reads fail too unless `id` is set, which they return.
    struct FailingBus {
        /// The byte every read returns, or `None` to fail reads.
        id: Option<u8>,
    }

    impl Bus for FailingBus {
        fn read_regs(&mut self, _: u32, _: &mut [u32]) -> Result<(), BusError> {
            unreachable!("the TPS6381x is on I2C")
        }

        fn write_regs(&mut self, _: u32, _: &[u32]) -> Result<(), BusError> {
            unreachable!("the TPS6381x is on I2C")
        }

        fn spi32(&mut self, _: SpiAddr, _: u32) -> Result<u32, BusError> {
            unreachable!("the TPS6381x is on I2C")
        }

        fn i2c(&mut self, _: I2cAddr, _: &[u8], read: &mut [u8]) -> Result<(), BusError> {
            match (self.id, read) {
                (Some(id), [byte]) => {
                    *byte = id;
                    Ok(())
                }
                _ => Err(BusError::Disconnected),
            }
        }

        fn sleep(&mut self, _: Duration) {}
    }

    /// Runs `init` on `bus` and returns the failed access as (chip, access, register).
    fn failed_access(bus: &mut FailingBus) -> (&'static str, Access, u32) {
        let voltage = OutputVoltage::from_millivolts(3450);
        match Tps6381x::usdr().init(bus, true, voltage) {
            Err(Error::Bus {
                chip, access, reg, ..
            }) => (chip, access, reg),
            other => panic!("expected a bus error, got {other:?}"),
        }
    }

    #[test]
    fn failed_id_read_names_the_devid_register() {
        let failure = failed_access(&mut FailingBus { id: None });
        assert_eq!(failure, (Tps6381x::NAME, Access::Read, 0x03));
    }

    #[test]
    fn failed_write_is_reported_as_a_write_not_an_id_read() {
        let failure = failed_access(&mut FailingBus {
            id: Some(Tps6381x::DEVICE_ID.to_raw()),
        });
        assert_eq!(
            failure,
            (Tps6381x::NAME, Access::Write, 0x01),
            "CONTROL is written first"
        );
    }
}
