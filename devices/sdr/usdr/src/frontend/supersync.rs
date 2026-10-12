//! The `SuperSync` clock board (libusdr's `supersync`, `device/ext_supersync/ext_supersync.c`):
//! an LMK5C33216 clock synchronizer, loaded with libusdr's fixed configuration.

use std::time::Duration;

use rsl_deps::tracing::debug;

use super::{FRONT_END_BUS, FrontEnd, Pin};
use crate::chips::lmk5c33216::Lmk5c33216;
use crate::error::Error;
use crate::fpga::{FrontEndGpio, PinMode};
use crate::lowlevel::{Bus, I2cAddr};

/// The board's name in logs (libusdr's `supersync`).
const NAME: &str = "supersync";

/// The clock synchronizer (`I2C_ADDR_LMK`).
const LMK: Lmk5c33216 = Lmk5c33216::at(I2cAddr::new(FRONT_END_BUS, 0x64));

/// Looks for the board and brings it up (`board_ext_supersync_init`): `Ok(None)` when its
/// clock synchronizer does not identify itself, otherwise the synchronizer gets libusdr's
/// register load.
///
/// # Errors
///
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
    LMK.load(bus)?;
    debug!(board = NAME, "front end initialized");
    Ok(Some(FrontEnd::SuperSync))
}
