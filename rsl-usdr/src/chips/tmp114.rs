//! TMP114 temperature sensor (source: `hw/tmp114/tmp114.c`).

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

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
