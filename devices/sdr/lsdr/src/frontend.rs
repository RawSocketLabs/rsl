//! The `LSDr`'s front-end probe (`usdr_device_m2_d09_4_ad45_2_initialize`'s
//! `device_fe_probe` with compatibility `lsdr`), run by `sdr-frontend`.
//!
//! The connector admits fe1005000 (its XRA1405 over SPI 4) and fe4ch4007200 (its sensor,
//! expanders and DAC on I2C core 1). Either comes up whole in the shared crate, as libusdr
//! brings it up.

use sdr_frontend::{Connector, FrontEnd, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the `LSDr`'s connector as libusdr's open does without an `fe=` hint, and keeps
/// the board that came up. A board that fails to come up is logged and skipped, as
/// libusdr does.
///
/// # Errors
///
/// None yet: probing automatically, a board that fails is skipped (`sdr-frontend`'s
/// errors are kept for the hints, which are not offered).
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    Ok(Connector::Lsdr.probe(bus, &FrontEndSelection::Auto)?)
}
