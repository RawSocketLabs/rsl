//! The XSDR's front-end board probe (`usdr_device_m2_lm7_1_initialize`'s `device_fe_probe`
//! with compatibility `m2a+e` on I2C bus 1), run by `sdr-frontend` as on the uSDR. Not
//! the RFIC's own RF front end (`portable::frontend`).
//!
//! The `PCIe` front end and the M.2-to-`PCIe` breakout come up whole in the shared crate.
//! XMASS, `SuperSync` and `SimpleSync` need the LMK05318 and LMK5C33216 bring-ups, which
//! `usdr` owns until they move to the shared crate: a board of theirs that answers
//! stops the open with [`Error::FrontEndNotPorted`] where libusdr would bring it up.

use sdr_frontend::{BringUp, Connector, Detected, Failure, FrontEndKind, FrontEndSelection};

use crate::error::Error;
use crate::lowlevel::Bus;

/// Probes the XSDR's connector as libusdr's open does without an `fe=` hint, and keeps the
/// board that came up.
///
/// # Errors
///
/// [`Error::FrontEndNotPorted`] for a board whose bring-up is not ported. A board that
/// fails to come up is skipped, as libusdr does.
pub(crate) fn probe(bus: &mut dyn Bus) -> Result<Option<FrontEndKind>, Error> {
    Connector::M2KeyAE.probe(bus, &FrontEndSelection::Auto, &mut XsdrBringUp)
}

/// The XSDR's part of the probe: the boards the shared crate brings up whole.
struct XsdrBringUp;

impl BringUp for XsdrBringUp {
    type FrontEnd = FrontEndKind;
    type Error = Error;

    fn bring_up(
        &mut self,
        _: &mut dyn Bus,
        detected: Detected,
    ) -> Result<FrontEndKind, Failure<Error>> {
        match detected {
            Detected::Pciefe(_) => Ok(FrontEndKind::Pciefe),
            Detected::Exm2pe(board) => Ok(FrontEndKind::Exm2pe {
                dac: board.has_dac(),
            }),
            other => Err(Failure::Stop(Error::FrontEndNotPorted(other.board()))),
        }
    }
}
