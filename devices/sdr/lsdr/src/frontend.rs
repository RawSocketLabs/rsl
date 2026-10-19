//! The `LSDr`'s front-end probe (`usdr_device_m2_d09_4_ad45_2_initialize`'s
//! `device_fe_probe` with compatibility `lsdr`), run by `sdr-frontend`.
//!
//! The connector admits fe1005000 (its XRA1405 over SPI 4) and fe4ch4007200 (its sensor
//! and expanders on I2C core 1). fe1005000 comes up whole in the shared crate, as libusdr
//! brings it up; fe4ch4007200's bring-up is not ported, so one that answers stops the open
//! with [`Error::FrontEndNotPorted`] where libusdr would bring it up.

use sdr_frontend::{Connector, FrontEnd, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the `LSDr`'s connector as libusdr's open does without an `fe=` hint, and keeps
/// the board that came up. A board that fails to come up is logged and skipped, as
/// libusdr does.
///
/// # Errors
///
/// [`Error::FrontEndNotPorted`] when fe4ch4007200 answers.
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    Connector::Lsdr
        .probe(bus, &FrontEndSelection::Auto)
        .map_err(|error| match error {
            sdr_frontend::Error::NotPorted(board) => Error::FrontEndNotPorted(board),
            other => other.into(),
        })
}
