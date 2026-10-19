//! Front-end boards on the uSDR's M.2 connector (libusdr's `device/device_fe.c` and its
//! `device/ext_*` boards).
//!
//! After power-up libusdr probes for a front end with the uSDR's compatibility string
//! `m2a+e`, trying each board in a fixed order until one comes up. A board reaches the
//! uSDR through the connector's GPIO pins (FPGA register 3) and the FPGA's I2C bus 1
//! (`I2C_BUS_FRONTEND`). The probe and every board's bring-up and controls are
//! `sdr-frontend`'s, shared with the other drivers whose devices have a front-end
//! connector; this module re-exports its types. Which boards the uSDR admits:
//!
//! | Board (libusdr name) | Admitted | Here |
//! |----------------------|----------|------|
//! | `PCIe` front end (`pciefe`) | yes | ported |
//! | XMASS (`xmass`) | yes | ported (LMK05318 bring-up, calibration LO and path) |
//! | `SuperSync` (`supersync`) | yes | ported (fixed register load) |
//! | `SimpleSync` (`simplesync`) | yes | ported (LMK05318 bring-up, measurement LO) |
//! | M.2-to-`PCIe` breakout (`exm2pe`) | yes | ported |
//! | 100–5000 MHz front end (`fe1005000`) | no: `LSDr` only | probing it accesses nothing |
//! | 4-channel 400–7200 MHz (`fe4ch4007200`) | tried, cannot work: needs I2C core 1 | the automatic probe tries it as libusdr does, and the uSDR's transports refuse its first transfer |
//!
//! The breakout has no ID and comes up whether or not its DAC answers, so when nothing
//! else does, libusdr reports a breakout without a DAC even with no board attached.
//!
//! [`xmass_firmware`] replaces the firmware of the XMASS board's `PCIe` switch, as
//! libusdr's `xmass_fwupd` tool does, through the `usdr_pcie_uram` driver's XMASS device.

pub mod xmass_firmware;

use sdr_frontend::Connector;
pub(crate) use sdr_frontend::FrontEnd;
pub use sdr_frontend::{
    ClockPlanError, Exm2peOptions, FrontEndKind, FrontEndSelection, PciefeAttenuation,
    PciefeDuplexer, PciefeOptions, PciefePath, PciefeRevision, PciefeRxFilter, PciefeSetting,
    PciefeTxFilter, XmassCalibrationPath,
};

use crate::error::Error;
use crate::lowlevel::Bus;

/// The boards with a calibration LO, for [`Error::FrontEndMissing`].
pub(crate) const SYNC_CALIBRATION_BOARDS: &str = "xmass or simplesync";

/// The XMASS board's name, for [`Error::FrontEndMissing`].
pub(crate) const XMASS: &str = "xmass";

/// Looks for a front end as `selection` says (`device_fe_probe` with compatibility `m2a+e`)
/// and brings up the first that answers.
///
/// A hinted board's failure fails the probe; while probing automatically, a failing board
/// is logged and skipped, as libusdr does. A hint that finds nothing is no error: the
/// uSDR's libusdr ignores the probe's `-ENODEV`.
pub(crate) fn probe(
    bus: &mut dyn Bus,
    selection: &FrontEndSelection,
) -> Result<Option<FrontEnd>, Error> {
    Ok(Connector::M2KeyAE.probe(bus, selection)?)
}
