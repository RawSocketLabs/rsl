//! The `SuperSync` clock board's identification (libusdr's `supersync`,
//! `device/ext_supersync/ext_supersync.c`): an LMK5C33216 clock synchronizer, which the
//! driver then loads with libusdr's fixed configuration.

use std::time::Duration;

use rsl_lowlevel::{Bus, I2cAddr};

use crate::FRONT_END_BUS;
use crate::chips::clock_id;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

/// The clock synchronizer (`I2C_ADDR_LMK`).
pub const LMK5C33216: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x64);

/// The synchronizer's name in errors.
const CHIP: &str = "LMK5C33216";

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

/// Looks for the board (`board_ext_supersync_init` up to its ID check): configures the
/// connector's pins, waits for the board to power up and reads the synchronizer's ID.
///
/// # Errors
///
/// Any bus failure.
pub(crate) fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    FrontEndGpio::run(bus, &SETUP)?;
    bus.sleep(POWER_UP);
    Ok(clock_id::read(bus, LMK5C33216, CHIP)? == ID)
}
