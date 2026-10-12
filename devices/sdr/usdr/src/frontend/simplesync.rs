//! The `SimpleSync` clock board (libusdr's `simplesync`,
//! `device/ext_simplesync/ext_simplesync.c`): an LMK05318 fanning a disciplined reference
//! out to four SDRs.
//!
//! Only detection is ported. Bringing the board up needs the LMK05318's configuration and
//! output solver, which are not ported, so a board that answers fails the open rather than
//! running half configured.

use std::time::Duration;

use super::{FRONT_END_BUS, FrontEnd, Pin};
use crate::chips::lmk05318::Lmk05318;
use crate::error::Error;
use crate::fpga::{FrontEndGpio, PinMode};
use crate::lowlevel::{Bus, I2cAddr};

/// The board's name in errors (libusdr's `simplesync`).
pub(super) const NAME: &str = "simplesync";

/// The clock synchronizer (`I2C_ADDR_LMK`).
const LMK: Lmk05318 = Lmk05318::at(I2cAddr::new(FRONT_END_BUS, 0x65));

/// The clock tree's power enable (`GPIO_ENCLKPWR`).
const EN_CLOCK_POWER: Pin = 5;
/// The GPS module's enable (`GPIO_ENGPS`).
const EN_GPS: Pin = 6;

/// Looks for the board (`board_ext_simplesync_init` up to `lmk05318_create`'s ID check);
/// `Ok(None)` when its clock synchronizer does not identify itself. The board's clock tree
/// and GPS are left powered, as libusdr leaves them.
///
/// # Errors
///
/// [`Error::UnsupportedFrontEnd`] if the board answers; any bus failure.
pub(super) fn init(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    // The connector's I2C lines to the core, the SYSREF generator out, and both enables.
    const CONFIG: [(Pin, PinMode); 5] = [
        (0, PinMode::Alternate0),
        (1, PinMode::Alternate0),
        (7, PinMode::Alternate0),
        (EN_CLOCK_POWER, PinMode::Output),
        (EN_GPS, PinMode::Output),
    ];
    for (pin, mode) in CONFIG {
        FrontEndGpio::configure(bus, pin, mode)?;
    }
    let enables = 1 << EN_CLOCK_POWER | 1 << EN_GPS;
    FrontEndGpio::drive(bus, enables, enables)?;
    // Wait for the board to power up.
    bus.sleep(Duration::from_millis(50));
    if LMK.read_id(bus)? != Lmk05318::ID {
        return Ok(None);
    }
    Err(Error::UnsupportedFrontEnd(NAME))
}
