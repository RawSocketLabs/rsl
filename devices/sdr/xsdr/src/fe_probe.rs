//! The XSDR's front-end board probe (`usdr_device_m2_lm7_1_initialize`'s `device_fe_probe`
//! with compatibility `m2a+e` on I2C bus 1), run by `sdr-frontend` as on the uSDR. Not
//! the RFIC's own RF front end (`portable::frontend`).
//!
//! Every board the connector admits comes up whole in the shared crate, as libusdr brings
//! it up: the `PCIe` front end, XMASS, `SuperSync`, `SimpleSync` and the M.2-to-`PCIe`
//! breakout. fe4ch4007200, whose bring-up the shared crate runs on the `LSDr`, cannot come
//! up on the XSDR: its chips sit on I2C core 1, which the XSDR's transports refuse (one
//! core, `rev_generic`), and the automatic probe reaches it only if the breakout fails.

use sdr_frontend::{Connector, FrontEnd, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the XSDR's connector as libusdr's open does with `selection` (its `fe=` option),
/// and keeps the board that came up. Probing automatically, a board that fails to come up
/// is logged and skipped and an empty connector is no error, as in libusdr.
///
/// # Errors
///
/// [`Error::NoFrontEnd`] when a hint (or [`FrontEndSelection::None`]) finds nothing,
/// libusdr's `-ENODEV`, which the XSDR's open does not ignore; [`Error::FrontEnd`] for the
/// hinted board's failure.
pub(crate) fn probe(
    bus: &mut dyn Bus,
    selection: &FrontEndSelection,
) -> Result<Option<FrontEnd>, Error> {
    let found = Connector::M2KeyAE.probe(bus, selection)?;
    if found.is_none() && *selection != FrontEndSelection::Auto {
        return Err(Error::NoFrontEnd);
    }
    Ok(found)
}
