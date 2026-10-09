//! TMP114 temperature sensor, which reads the board temperature for the thermal policy.
//!
//! # What it does
//!
//! The TMP114 is a small digital thermometer. It converts on its own, continuously, and
//! holds the latest reading in a result register. A read returns that last result, so it
//! never waits on a conversion. Until the first conversion completes the result reads
//! 0 °C, which looks cool; the driver assumes one has completed by the time it reads.
//! It measures the board where it is mounted, not the die of
//! any other chip: the FPGA or LMS6002D can be hotter than it reports.
//!
//! The TMP114 comes in variants with different fixed I2C addresses; the uSDR
//! carries the TMP114NB, at 0x4E.
//!
//! # How the driver uses it
//!
//! It is the first chip the driver talks to after reading the board revision.
//! [`Tmp114::check_id`] confirms a TMP114 answers, because a missing sensor would read as
//! −0.0078 °C (0xFFFF) and look cool. [`Tmp114::celsius`] then feeds the thermal policy,
//! before anything else is powered and again whenever the caller asks.
//!
//! # Sources
//!
//! Register meanings: TI SNIS214E, "TMP114", §8.6. Its registers are 16 bits wide and sent
//! MSB first.

use bnb::{BitEnum, bitfield, u4, u12};

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Reg {
    /// The latest conversion: two's complement, 1/128 °C (0.0078125 °C) per bit.
    /// `Temp_Result`, §8.6.1; reset 0, until the first conversion completes.
    Temperature = 0x00,
    /// See [`DeviceId`]. `Device_ID`, §8.6.12.
    DeviceId = 0x0b,
}

/// The part and its revision. `Device_ID`, §8.6.12 (read-only).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceId {
    /// Device revision. `Rev`, bits 15:12; 1 on the TMP114.
    #[bits(12..=15)]
    revision: u4,
    /// Device ID. `ID`, bits 11:0; 0x114 on the TMP114.
    #[bits(0..=11)]
    device: u12,
}

/// Result register counts per degree Celsius.
const COUNTS_PER_CELSIUS: f32 = 128.0;

/// A TMP114 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tmp114 {
    /// Where the sensor answers.
    dev: I2cAddr,
}

impl Tmp114 {
    /// The ID libusdr requires: device 0x114 (TMP114), revision 1. An idle bus reads 0xFFFF.
    const DEVICE_ID: u16 = 0x1114;

    /// The sensor at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// The sensor where the uSDR wires it: FPGA I2C bus 0, address 0x4E, the TMP114NB variant's
    /// fixed address (`I2C_DEV_TMP114NB` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x4e))
    }

    /// Reads the temperature in degrees Celsius.
    pub(crate) fn celsius(self, bus: &mut dyn Bus) -> Result<f32, Error> {
        let raw = self.read(bus, Reg::Temperature, "TMP114 temperature read")?;
        Ok(f32::from(i16::from_be_bytes(raw)) / COUNTS_PER_CELSIUS)
    }

    /// Checks the device ID register.
    pub(crate) fn check_id(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let raw = self.read(bus, Reg::DeviceId, "TMP114 ID read")?;
        let id = DeviceId::from_raw(u16::from_be_bytes(raw));
        if !(id.device().value() == 0x114 && id.revision().value() == 1) {
            return Err(Error::ChipId {
                chip: "TMP114",
                expected: Self::DEVICE_ID.into(),
                found: id.to_raw().into(),
            });
        }
        Ok(())
    }

    /// Reads one 16-bit register, in wire order.
    fn read(self, bus: &mut dyn Bus, reg: Reg, op: &'static str) -> Result<[u8; 2], Error> {
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg.into()], &mut raw).during(op)?;
        Ok(raw)
    }
}
