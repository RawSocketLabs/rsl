//! The `LSDr`'s front-end probe (`usdr_device_m2_d09_4_ad45_2_initialize`'s
//! `device_fe_probe` with compatibility `lsdr`), run by `sdr-frontend`.
//!
//! The connector admits fe1005000 (its XRA1405 over SPI 4) and fe4ch4007200 (its sensor,
//! expanders and DAC on I2C core 1). Either comes up whole in the shared crate, as libusdr
//! brings it up; fe4ch4007200 only on a transport that passes I2C core 1. The `LSDr`
//! declares one I2C core (`DNLL_I2C_COUNT` 1), and the upstream driver likely refuses
//! core 1 (`AGENTS.md`, open items): only the oracle shim and the sim bus pass it.

use sdr_frontend::{Connector, FrontEnd, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the `LSDr`'s connector as libusdr's open does without an `fe=` hint, and keeps
/// the board that came up. A board that fails to come up is logged and skipped, as
/// libusdr does.
///
/// # Errors
///
/// [`Error::FrontEnd`] when `sdr-frontend`'s probe fails. Probing automatically it logs
/// and skips every board that fails, so it reports none today; the `Result` stays so a
/// hinted probe, should one be offered, can fail open as libusdr's does.
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    Ok(Connector::Lsdr.probe(bus, &FrontEndSelection::Auto)?)
}
