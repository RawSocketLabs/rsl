//! The XSDR's front-end board probe (`usdr_device_m2_lm7_1_initialize`'s `device_fe_probe`
//! with compatibility `m2a+e` on I2C bus 1), run by `sdr-frontend` as on the uSDR. Not
//! the RFIC's own RF front end (`portable::frontend`).
//!
//! Every board the connector admits comes up whole in the shared crate, as libusdr brings
//! it up: the `PCIe` front end, XMASS, `SuperSync`, `SimpleSync` and the M.2-to-`PCIe`
//! breakout. fe4ch4007200, the one board the shared crate does not bring up, cannot come
//! up on the XSDR: its chips sit on I2C core 1, which the XSDR's transports refuse (one
//! core, `rev_generic`), and the automatic probe reaches it only if the breakout fails.

use sdr_frontend::{Connector, FrontEnd, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the XSDR's connector as libusdr's open does without an `fe=` hint, and keeps the
/// board that came up. A board that fails to come up is logged and skipped, as libusdr
/// does.
///
/// # Errors
///
/// Only a bus failure the probe does not skip; open never sees one from an automatic
/// probe.
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<Option<FrontEnd>, Error> {
    Ok(Connector::M2KeyAE.probe(bus, &FrontEndSelection::Auto)?)
}
