//! The four-channel 400–7200 MHz front end (libusdr's `fe4ch4007200`,
//! `device/ext_fe_ch4_400_7200/ext_fe_ch4_400_7200.c`), built for the sSDR and dSDR.
//!
//! libusdr applies no compatibility filter, so its probe tries it on the uSDR too: last,
//! after the breakout, which only a bus failure stops. It talks to its chips through FPGA
//! I2C core 1, which the uSDR lacks: both transports reject the first such transfer with
//! `-EINVAL` (the `PCIe` kernel driver's and USB transport's `instance >= i2c_count`
//! checks, read in the upstream tree at the pinned commit outside the vendored subset; the
//! uSDR declares one I2C core, `/ll/i2c/0`). rsl-usdr has no I2C core index to send, so it
//! fails right after configuring the connector's pins, before that transfer. No selection
//! names this board.

use super::{FrontEnd, Pin};
use crate::error::Error;
use crate::fpga::{FrontEndGpio, PinMode};
use crate::lowlevel::Bus;

/// The board's name in errors (libusdr's `fe4ch4007200`).
pub(super) const NAME: &str = "fe4ch4007200";

/// Configures the pins for the board's second and third I2C buses, then fails, as
/// libusdr's first transfer on I2C core 1 does (`ext_fe_ch4_400_7200_init`).
///
/// # Errors
///
/// [`Error::FrontEnd`] after the pin configuration; any bus failure before it.
pub(super) fn init(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    // I2C3 clock, then I2C4 data and clock.
    const CONFIG: [Pin; 3] = [6, 1, 5];
    for pin in CONFIG {
        FrontEndGpio::configure(bus, pin, PinMode::Alternate1)?;
    }
    Err(Error::FrontEnd {
        board: NAME,
        reason: "it needs FPGA I2C core 1, which the uSDR lacks",
    })
}
