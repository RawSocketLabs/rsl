//! The `SuperSync` clock board (libusdr's `supersync`,
//! `device/ext_supersync/ext_supersync.c`): an LMK5C33216 clock synchronizer, loaded with
//! libusdr's fixed configuration once it has identified itself.

use std::time::Duration;

use deps::tracing::debug;
use hal::{Bus, I2cAddr};

use crate::FRONT_END_BUS;
use crate::chips::clock_id;
use crate::chips::lmk5c33216::Lmk5c33216;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

/// The clock synchronizer (`I2C_ADDR_LMK`).
const LMK5C33216: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x64);

/// The LMK5C33216's vendor ID (two bytes), product ID and revision, in wire order.
const ID: [u8; 4] = [0x10, 0x0b, 0x40, 0x05];

/// The connector setup: its I2C lines as inputs, then 1PPS in.
const SETUP: [GpioCommand; 3] = [
    GpioCommand::Configure(0, PinMode::Input),
    GpioCommand::Configure(1, PinMode::Input),
    GpioCommand::Configure(2, PinMode::Alternate0),
];

/// How long libusdr lets the board power up before reading the synchronizer's ID.
const POWER_UP: Duration = Duration::from_millis(50);

/// A `SuperSync` board that came up. libusdr keeps no state for it beyond the chip's
/// address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SuperSync;

impl SuperSync {
    /// Looks for the board and brings it up (`board_ext_supersync_init`): configures the
    /// connector's pins, waits for the board to power up, reads the synchronizer's ID and
    /// writes libusdr's register load. `None` when the ID is not the LMK5C33216's.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Option<Self>, Error> {
        FrontEndGpio::run(bus, &SETUP)?;
        bus.sleep(POWER_UP);
        if clock_id::read(bus, LMK5C33216, Lmk5c33216::NAME)? != ID {
            return Ok(None);
        }
        Lmk5c33216::at(LMK5C33216).load(bus)?;
        debug!(board = "supersync", "front end initialized");
        Ok(Some(Self))
    }
}
