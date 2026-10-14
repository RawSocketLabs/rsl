//! The `LSDr`'s front-end probe (`usdr_device_m2_d09_4_ad45_2_initialize`'s
//! `device_fe_probe` with compatibility `lsdr`), run by `sdr-frontend`.
//!
//! The connector admits fe1005000 (its XRA1405 over SPI 4) and fe4ch4007200 (its sensor
//! and expanders on I2C core 1); neither board's bring-up is ported, so one that answers
//! stops the open with [`Error::FrontEndNotPorted`] where libusdr would bring it up.

use sdr_frontend::{BringUp, Connector, Detected, Failure, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the `LSDr`'s connector as libusdr's open does without an `fe=` hint.
///
/// # Errors
///
/// [`Error::FrontEndNotPorted`] when a board answers. A board that fails to come up is
/// skipped, as libusdr does.
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<(), Error> {
    match Connector::Lsdr.probe(bus, &FrontEndSelection::Auto, &mut LsdrBringUp)? {
        Some(never) => match never {},
        None => Ok(()),
    }
}

/// The `LSDr`'s part of the probe: no bring-up is ported.
struct LsdrBringUp;

/// No front end comes up on the `LSDr` yet.
enum Never {}

impl BringUp for LsdrBringUp {
    type FrontEnd = Never;
    type Error = Error;

    fn bring_up(&mut self, _: &mut dyn Bus, detected: Detected) -> Result<Never, Failure<Error>> {
        Err(Failure::Stop(Error::FrontEndNotPorted(detected.board())))
    }
}
