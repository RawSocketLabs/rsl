//! The identification TI's clock synchronizers share (LMK05318, LMK5C33216): vendor ID,
//! product ID and revision in the four registers from address 0, behind a 16-bit register
//! address.

use hal::{Bus, I2cAddr};

use crate::error::{BusContext, Error};

/// The identification registers' first address (`VNDRID_BY1`).
const ID: u16 = 0x0000;

/// Reads the four identification bytes of the clock synchronizer `chip` at `dev`, in wire
/// order (`lmk05318_reg_get_u32` and `lmk_5c33216_reg_u32` at address 0).
///
/// # Errors
///
/// Any bus failure.
pub(crate) fn read(bus: &mut dyn Bus, dev: I2cAddr, chip: &'static str) -> Result<[u8; 4], Error> {
    let mut raw = [0; 4];
    bus.i2c(dev, &ID.to_be_bytes(), &mut raw)
        .reading(chip, ID)?;
    Ok(raw)
}
