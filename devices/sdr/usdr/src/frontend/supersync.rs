//! The `SuperSync` clock board (libusdr's `supersync`, `device/ext_supersync/ext_supersync.c`):
//! an LMK5C33216 clock synchronizer.

use std::time::Duration;

use super::{FRONT_END_BUS, FrontEnd, Pin};
use crate::chips::lmk5c33216::Lmk5c33216;
use crate::error::Error;
use crate::fpga::{FrontEndGpio, PinMode};
use crate::lowlevel::{Bus, I2cAddr};

/// The board's name in errors (libusdr's `supersync`).
pub(super) const NAME: &str = "supersync";

/// The clock synchronizer (`I2C_ADDR_LMK`).
const LMK: Lmk5c33216 = Lmk5c33216::at(I2cAddr::new(FRONT_END_BUS, 0x64));

/// Looks for the board and brings it up (`board_ext_supersync_init`); `Ok(None)` when its
/// clock synchronizer does not identify itself.
///
/// # Errors
///
/// [`Error::UnsupportedFrontEnd`] if the board answers: its register load is not ported.
/// Any bus failure.
pub(super) fn init(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    // The connector's I2C lines as inputs, then 1PPS in.
    const CONFIG: [(Pin, PinMode); 3] = [
        (0, PinMode::Input),
        (1, PinMode::Input),
        (2, PinMode::Alternate0),
    ];
    for (pin, mode) in CONFIG {
        FrontEndGpio::configure(bus, pin, mode)?;
    }
    // Wait for the board to power up.
    bus.sleep(Duration::from_millis(50));
    if LMK.read_id(bus)? != Lmk5c33216::ID {
        return Ok(None);
    }
    Err(Error::UnsupportedFrontEnd(NAME))
}
