//! TMP114 temperature sensor (source: `hw/tmp114/tmp114.c`).

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Temperature result register: two's complement, 1/128 °C per bit.
const TEMP_RESULT: u8 = 0x00;
/// Result register counts per degree Celsius.
const COUNTS_PER_CELSIUS: f32 = 128.0;
/// Device ID register.
const DEVICE_ID: u8 = 0x0b;
/// The ID a TMP114 reports.
const TMP114_ID: u16 = 0x1114;

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
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[TEMP_RESULT], &mut raw)
            .during("TMP114 temperature read")?;
        Ok(f32::from(i16::from_be_bytes(raw)) / COUNTS_PER_CELSIUS)
    }

    /// Checks the device ID register (16 bits, sent MSB first).
    pub(crate) fn check_id(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let mut id = [0; 2];
        bus.i2c(self.dev, &[DEVICE_ID], &mut id)
            .during("TMP114 ID read")?;
        let found = u16::from_be_bytes(id);
        (found == TMP114_ID).then_some(()).ok_or(Error::ChipId {
            chip: "TMP114",
            expected: TMP114_ID.into(),
            found: found.into(),
        })
    }
}
