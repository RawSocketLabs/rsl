//! The `SimpleSync` clock board's identification (libusdr's `simplesync`,
//! `device/ext_simplesync/ext_simplesync.c`): an LMK05318 fanning a disciplined reference
//! out to four SDRs, which the driver then brings up.

use std::time::Duration;

use rsl_lowlevel::{Bus, I2cAddr};

use crate::FRONT_END_BUS;
use crate::chips::clock_id;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, Pin, PinMode};

/// The clock synchronizer (`I2C_ADDR_LMK`).
pub const LMK05318: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x65);

/// The clock tree's power enable (`GPIO_ENCLKPWR`).
const EN_CLOCK_POWER: Pin = 5;
/// The GPS module's enable (`GPIO_ENGPS`).
const EN_GPS: Pin = 6;
/// Both enables.
const ENABLES: u16 = 1 << EN_CLOCK_POWER | 1 << EN_GPS;

/// The connector setup: its I2C lines to the core, the SYSREF generator out, both enables
/// as outputs, then both driven high.
const SETUP: [GpioCommand; 6] = [
    GpioCommand::Configure(0, PinMode::Alternate0),
    GpioCommand::Configure(1, PinMode::Alternate0),
    GpioCommand::Configure(7, PinMode::Alternate0),
    GpioCommand::Configure(EN_CLOCK_POWER, PinMode::Output),
    GpioCommand::Configure(EN_GPS, PinMode::Output),
    GpioCommand::Drive {
        mask: ENABLES,
        levels: ENABLES,
    },
];

/// How long libusdr lets the board power up before reading the synchronizer's ID.
const POWER_UP: Duration = Duration::from_millis(50);

/// Looks for the board (`board_ext_simplesync_init` up to `lmk05318_create`'s ID check):
/// configures the connector's pins, powers the clock tree and GPS, waits and reads the
/// synchronizer's ID. An absent board stays powered, as libusdr leaves it.
///
/// # Errors
///
/// Any bus failure.
pub(crate) fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    FrontEndGpio::run(bus, &SETUP)?;
    bus.sleep(POWER_UP);
    Ok(clock_id::read(bus, LMK05318, rsl_lmk05318::NAME)? == rsl_lmk05318::ID)
}
