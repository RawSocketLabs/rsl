//! Front-end boards on the uSDR's M.2 connector (libusdr's `device/device_fe.c` and its
//! `device/ext_*` boards).
//!
//! After power-up libusdr probes for a front end with the uSDR's compatibility string
//! `m2a+e`, trying each board in a fixed order until one comes up. A board reaches the
//! uSDR through the connector's GPIO pins (FPGA register 3) and the FPGA's I2C bus 1
//! (`I2C_BUS_FRONTEND`). The probe, every board's identification, and the `PCIe` front end
//! and breakout whole, are `rsl-sdr-frontend`'s, shared with the other drivers whose
//! devices have a front-end connector; the bring-ups that need the LMK05318 or LMK5C33216
//! are here. Which boards the uSDR admits:
//!
//! | Board (libusdr name) | Admitted | Here |
//! |----------------------|----------|------|
//! | `PCIe` front end (`pciefe`) | yes | ported (`rsl-sdr-frontend`) |
//! | XMASS (`xmass`) | yes | ported (LMK05318 bring-up, calibration LO and path) |
//! | `SuperSync` (`supersync`) | yes | ported (fixed register load) |
//! | `SimpleSync` (`simplesync`) | yes | ported (LMK05318 bring-up, measurement LO) |
//! | M.2-to-`PCIe` breakout (`exm2pe`) | yes | ported (`rsl-sdr-frontend`) |
//! | 100–5000 MHz front end (`fe1005000`) | no: `LSDr` only | not offered; probing it accesses nothing |
//! | 4-channel 400–7200 MHz (`fe4ch4007200`) | tried, cannot work: needs I2C core 1 | not offered; the automatic probe tries it as libusdr does, and the uSDR's transports refuse its first transfer |
//!
//! The last two stay in the automatic probe's order for fidelity, but no selection names
//! them: neither can come up on a uSDR.
//!
//! The breakout has no ID and comes up whether or not its DAC answers, so when nothing
//! else does, libusdr reports a breakout without a DAC even with no board attached.

mod simplesync;
mod supersync;
mod xmass;

use rsl_deps::tracing::{debug, warn};
use rsl_sdr_frontend::{Board, BringUp, Connector, Detected, Exm2pe, Failure, Pciefe};
pub use rsl_sdr_frontend::{
    Exm2peOptions, FrontEndKind, FrontEndSelection, PciefeAttenuation, PciefeDuplexer,
    PciefeOptions, PciefePath, PciefeRevision, PciefeRxFilter, PciefeSetting, PciefeTxFilter,
    XmassCalibrationPath,
};

pub(crate) use self::simplesync::SimpleSync;
pub(crate) use self::xmass::Xmass;
pub use crate::chips::lmk05318::plan::ClockPlanError;
use crate::error::Error;
use crate::lowlevel::Bus;

/// The FPGA I2C bus on the M.2 connector (libusdr's `I2C_BUS_FRONTEND`).
const FRONT_END_BUS: u8 = 1;

/// A front end that came up, with its driver state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrontEnd {
    /// The `PCIe` front end.
    Pciefe(Pciefe),

    /// The M.2-to-`PCIe` breakout.
    Exm2pe(Exm2pe),

    /// The XMASS board; boxed, its clock synchronizer's state is large.
    Xmass(Box<Xmass>),

    /// The `SuperSync` clock board; libusdr keeps no state for it beyond the chip address.
    SuperSync,

    /// The `SimpleSync` clock board; boxed as XMASS.
    SimpleSync(Box<SimpleSync>),
}

impl FrontEnd {
    /// What kind of board this is.
    pub(crate) fn kind(&self) -> FrontEndKind {
        match self {
            Self::Pciefe(_) => FrontEndKind::Pciefe,
            Self::Exm2pe(board) => FrontEndKind::Exm2pe {
                dac: board.has_dac(),
            },
            Self::Xmass(_) => FrontEndKind::Xmass,
            Self::SuperSync => FrontEndKind::SuperSync,
            Self::SimpleSync(_) => FrontEndKind::SimpleSync,
        }
    }

    /// Sets the oscillator DAC (`dev_m2_lm6_1_sdr_vctcxo_set`): the `PCIe` front end's, or
    /// the breakout's if it answered. Other boards have none (libusdr's `-EINVAL`).
    pub(crate) fn set_oscillator_dac(&self, bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        match self {
            Self::Pciefe(_) => Ok(Pciefe::set_dac(bus, code)?),
            Self::Exm2pe(board) => Ok(board.set_dac(bus, code)?),
            Self::Xmass(_) | Self::SuperSync | Self::SimpleSync(_) => Err(Error::NoOscillatorDac),
        }
    }

    /// Retunes the synchronization board's calibration or measurement LO
    /// (`/dm/sync/cal/freq`): XMASS's output 4, `SimpleSync`'s outputs 0 to 3.
    pub(crate) fn set_sync_calibration_hz(
        &mut self,
        bus: &mut dyn Bus,
        hz: u32,
    ) -> Result<(), Error> {
        match self {
            Self::Xmass(board) => board.tune_calibration(bus, hz),
            Self::SimpleSync(board) => board.tune_lo(bus, hz),
            Self::Pciefe(_) | Self::Exm2pe(_) | Self::SuperSync => {
                Err(Error::FrontEndMissing(SYNC_CALIBRATION_BOARDS))
            }
        }
    }
}

/// The boards with a calibration LO, for [`Error::FrontEndMissing`].
pub(crate) const SYNC_CALIBRATION_BOARDS: &str = "xmass or simplesync";

/// The XMASS board's name, for [`Error::FrontEndMissing`].
pub(crate) const XMASS: &str = xmass::NAME;

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
    let front_end = Connector::M2KeyAE.probe(bus, selection, &mut UsdrBringUp)?;
    if *selection == FrontEndSelection::Auto {
        let kind = front_end.as_ref().map(FrontEnd::kind);
        debug!(?kind, "front-end probe finished");
    }
    Ok(front_end)
}

/// The uSDR's part of the probe: the bring-ups that need its LMK drivers.
struct UsdrBringUp;

impl BringUp for UsdrBringUp {
    type FrontEnd = FrontEnd;
    type Error = Error;

    fn bring_up(
        &mut self,
        bus: &mut dyn Bus,
        detected: Detected,
    ) -> Result<FrontEnd, Failure<Error>> {
        match detected {
            Detected::Pciefe(board) => Ok(FrontEnd::Pciefe(board)),
            Detected::Exm2pe(board) => Ok(FrontEnd::Exm2pe(board)),
            Detected::Xmass => Xmass::bring_up(bus).map(FrontEnd::from),
            Detected::SuperSync => supersync::bring_up(bus).map(|()| FrontEnd::SuperSync),
            Detected::SimpleSync => SimpleSync::bring_up(bus).map(FrontEnd::from),
            // The connector does not admit fe1005000, and fe4ch4007200's first transfer
            // needs an I2C core the uSDR lacks.
            other => Err(Error::FrontEnd {
                board: other.board().name(),
                reason: "it cannot come up on the uSDR",
            }),
        }
        .map_err(Failure::Board)
    }

    fn skipped(&mut self, board: Board, error: &Error) {
        warn!(board = board.name(), %error, "front end failed to come up; skipped");
    }
}
