//! The XMASS synchronization board (libusdr's `xmass`, `device/ext_xmass/ext_xmass.c`): an
//! LMK05318 clock tree, a calibration source and power amplifiers behind a GPIO expander.
//!
//! Only detection is ported. libusdr identifies the board by its expander: it writes a
//! direction mask and reads it back. Bringing the board up needs the LMK05318's
//! configuration and output solver, which are not ported, so a board that answers fails
//! the open rather than running half configured.

use super::{FRONT_END_BUS, FrontEnd, Pin};
use crate::chips::tca9555::{Reg, Tca9555};
use crate::error::Error;
use crate::fpga::{FrontEndGpio, PinMode};
use crate::lowlevel::{Bus, I2cAddr};

/// The board's name in errors (libusdr's `xmass`).
pub(super) const NAME: &str = "xmass";

/// The control expander, an `XRA1201` driven as a TCA9555 (`I2C_ADDR_XRA1201`).
const EXPANDER: Tca9555 = Tca9555::at(I2cAddr::new(FRONT_END_BUS, 0x14));

/// The direction mask libusdr writes and expects back: port 1's pins 13 to 15 inputs
/// (the RTS lines), every other pin an output (`out_msk`).
const DIRECTIONS: u16 = 0xe000;

/// Looks for the board (`board_xmass_init` up to its expander check); `Ok(None)` when it
/// is absent.
///
/// # Errors
///
/// [`Error::UnsupportedFrontEnd`] if the board answers; any bus failure.
pub(super) fn init(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    // The connector's I2C lines as inputs, then 1PPS in and the SYSREF generator out.
    const CONFIG: [(Pin, PinMode); 4] = [
        (0, PinMode::Input),
        (1, PinMode::Input),
        (2, PinMode::Alternate0),
        (3, PinMode::Alternate0),
    ];
    for (pin, mode) in CONFIG {
        FrontEndGpio::configure(bus, pin, mode)?;
    }
    // libusdr reads the directions before writing them, and ignores that first value.
    EXPANDER.read16(bus, Reg::Config0)?;
    EXPANDER.write16(bus, Reg::Config0, DIRECTIONS)?;
    if EXPANDER.read16(bus, Reg::Config0)? != DIRECTIONS {
        return Ok(None);
    }
    Err(Error::UnsupportedFrontEnd(NAME))
}
