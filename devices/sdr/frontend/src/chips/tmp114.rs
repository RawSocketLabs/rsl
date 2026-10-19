//! TMP114 temperature sensor, as the front-end boards carry it: XMASS revision 1a's two,
//! whose readings libusdr only logs.
//!
//! # What it does
//!
//! A digital thermometer converting on its own; the result register holds the latest
//! reading. Its registers are 16 bits wide behind an 8-bit pointer, sent MSB first.
//!
//! # Sources
//!
//! Register addresses: libusdr's `hw/tmp114/tmp114.c` (`enum tmp114_regs`) and
//! `TMP114_DEVICE_ID`; the result's scale (signed, 1/128 °C per count) TI SNIS214E §8.6.1.
//! `usdr` keeps its own TMP114 driver for the uSDR's board sensor (its portable tree
//! and native executor use it); this one is the few reads the boards need, kept small.

use hal::{Bus, I2cAddr};

use crate::error::{BusContext, Error};

/// Register addresses (`enum tmp114_regs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Reg {
    /// The latest temperature, signed, 1/128 °C per count (`TMP114_Temp_Result`).
    Temperature = 0x00,

    /// Device and revision (`TMP114_Device_ID`).
    DeviceId = 0x0b,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// A TMP114 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tmp114 {
    /// Where the sensor answers.
    dev: I2cAddr,
}

impl Tmp114 {
    /// The chip's name in errors.
    const NAME: &'static str = "TMP114";

    /// The ID libusdr requires (`TMP114_DEVICE_ID`): device 0x114, revision 1. An idle bus
    /// reads 0xFFFF.
    pub(crate) const DEVICE_ID: u16 = 0x1114;

    /// The sensor at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Whether the device ID register reads the ID libusdr requires (`tmp114_devid_get`
    /// compared with `TMP114_DEVICE_ID`); a missing sensor reads `false`.
    pub(crate) fn identifies(self, bus: &mut dyn Bus) -> Result<bool, Error> {
        Ok(self.read(bus, Reg::DeviceId)? == Self::DEVICE_ID)
    }

    /// Reads the temperature in degrees Celsius (`tmp114_temp_get`).
    pub(crate) fn celsius(self, bus: &mut dyn Bus) -> Result<f32, Error> {
        let raw = self.read(bus, Reg::Temperature)?;
        Ok(f32::from(i16::from_be_bytes(raw.to_be_bytes())) / 128.0)
    }

    /// Reads one 16-bit register, MSB first (`tmp114_reg_get`).
    fn read(self, bus: &mut dyn Bus, reg: Reg) -> Result<u16, Error> {
        let reg = u8::from(reg);
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(u16::from_be_bytes(raw))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use hal::{BusError, SpiAddr};

    use super::*;

    /// A sensor answering every read with `wire`.
    struct Answering([u8; 2]);

    impl Bus for Answering {
        fn read_regs(&mut self, _: u32, _: &mut [u32]) -> Result<(), BusError> {
            Err(BusError::Unsupported("registers"))
        }
        fn write_regs(&mut self, _: u32, _: &[u32]) -> Result<(), BusError> {
            Err(BusError::Unsupported("registers"))
        }
        fn spi32(&mut self, _: SpiAddr, _: u32) -> Result<u32, BusError> {
            Err(BusError::Unsupported("SPI"))
        }
        fn i2c(&mut self, _: I2cAddr, _: &[u8], read: &mut [u8]) -> Result<(), BusError> {
            read.copy_from_slice(&self.0);
            Ok(())
        }
        fn sleep(&mut self, _: Duration) {}
    }

    #[test]
    fn temperatures_follow_the_datasheets_signed_scale() {
        // SNIS214E §8.6.1: signed big-endian counts, 1/128 °C per count.
        let sensor = Tmp114::at(I2cAddr::new(1, 0x4f));
        for (wire, expected) in [([0x0c, 0x80], 25.0), ([0xe4, 0x80], -55.0)] {
            let celsius = sensor.celsius(&mut Answering(wire)).expect("reads");
            assert!((celsius - expected).abs() < f32::EPSILON, "wire {wire:?}");
        }
    }

    #[test]
    fn only_libusdrs_device_id_identifies() {
        let sensor = Tmp114::at(I2cAddr::new(1, 0x4f));
        assert!(
            sensor
                .identifies(&mut Answering([0x11, 0x14]))
                .expect("reads")
        );
        assert!(
            !sensor
                .identifies(&mut Answering([0xff, 0xff]))
                .expect("reads")
        );
    }
}
