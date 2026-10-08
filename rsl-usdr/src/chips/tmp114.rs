//! TMP114 temperature sensor (source: `hw/tmp114/tmp114.c`). Its registers are 16 bits wide
//! and sent MSB first.

use bnb::BitEnum;

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Reg {
    /// Temperature result: two's complement, 1/128 °C per bit.
    TempResult = 0x00,
    /// Device ID; see [`DeviceId`].
    DeviceId = 0x0b,
}

/// The device ID register.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u16)]
#[repr(u16)]
enum DeviceId {
    /// TMP114.
    Tmp114 = 0x1114,
    /// Any other part, or no part (an idle bus reads `0xffff`).
    #[catch_all]
    Other(u16),
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
    /// The sensor at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Reads the temperature in degrees Celsius.
    pub(crate) fn celsius(self, bus: &mut dyn Bus) -> Result<f32, Error> {
        let raw = self.read(bus, Reg::TempResult, "TMP114 temperature read")?;
        Ok(f32::from(i16::from_be_bytes(raw)) / COUNTS_PER_CELSIUS)
    }

    /// Checks the device ID register.
    pub(crate) fn check_id(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let raw = self.read(bus, Reg::DeviceId, "TMP114 ID read")?;
        match DeviceId::from(u16::from_be_bytes(raw)) {
            DeviceId::Tmp114 => Ok(()),
            DeviceId::Other(found) => Err(Error::ChipId {
                chip: "TMP114",
                expected: u16::from(DeviceId::Tmp114).into(),
                found: found.into(),
            }),
        }
    }

    /// Reads one 16-bit register, in wire order.
    fn read(self, bus: &mut dyn Bus, reg: Reg, op: &'static str) -> Result<[u8; 2], Error> {
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg.into()], &mut raw).during(op)?;
        Ok(raw)
    }
}
