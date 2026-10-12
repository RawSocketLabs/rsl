//! Front-end boards on the uSDR's M.2 connector (libusdr's `device/device_fe.c` and its
//! `device/ext_*` boards).
//!
//! After power-up libusdr probes for a front end with the uSDR's compatibility string
//! `m2a+e`, trying each board in a fixed order until one comes up. A board reaches the
//! uSDR through the connector's GPIO pins (FPGA register 3, [`FrontEndGpio`]) and the
//! FPGA's I2C bus 1 (`I2C_BUS_FRONTEND`). Which boards the uSDR admits:
//!
//! | Board (libusdr name) | Admitted | Here |
//! |----------------------|----------|------|
//! | `PCIe` front end (`pciefe`) | yes | ported |
//! | XMASS (`xmass`) | yes | detected; bring-up not ported |
//! | `SuperSync` (`supersync`) | yes | detected; bring-up not ported |
//! | `SimpleSync` (`simplesync`) | yes | detected; bring-up not ported |
//! | M.2-to-`PCIe` breakout (`exm2pe`) | yes | ported |
//! | 100–5000 MHz front end (`fe1005000`) | no: `LSDr` only | no bus access, as in libusdr |
//! | 4-channel 400–7200 MHz (`fe4ch4007200`) | tried, fails: needs I2C core 1 | fails as libusdr does |
//!
//! The breakout has no ID and comes up whether or not its DAC answers, so when nothing
//! else does, libusdr reports a breakout without a DAC even with no board attached.
//!
//! These sequences interleave connector-GPIO register writes with I2C transfers wider than
//! one byte (four-byte reads, 16-bit register addresses, three-byte writes), which the
//! portable `Sequence` seam does not express, so they run as blocking code over [`Bus`].
//!
//! [`FrontEndGpio`]: crate::fpga::FrontEndGpio

mod exm2pe;
mod fe4ch;
mod pciefe;
mod simplesync;
mod supersync;
mod xmass;

use rsl_deps::tracing::{debug, warn};

pub(crate) use self::exm2pe::Exm2pe;
pub use self::exm2pe::Exm2peOptions;
pub(crate) use self::pciefe::Pciefe;
pub use self::pciefe::{
    PciefeAttenuation, PciefeDuplexer, PciefeOptions, PciefePath, PciefeRevision, PciefeRxFilter,
    PciefeSetting, PciefeTxFilter,
};
use crate::error::Error;
use crate::lowlevel::Bus;

/// The FPGA I2C bus on the M.2 connector (libusdr's `I2C_BUS_FRONTEND`).
const FRONT_END_BUS: u8 = 1;

/// A pin of the connector's GPIO core, 0 to 14.
type Pin = u8;

/// Which front end to look for after power-up: libusdr's `fe=` option, typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEndSelection {
    /// Try every board in libusdr's order and keep the first that comes up; a board that
    /// fails is skipped. libusdr's default (no `fe=`).
    #[default]
    Auto,

    /// Probe nothing. libusdr does this for an `fe=` naming no board (`fe=none`).
    None,

    /// Only the `PCIe` front end, with these options; a failure fails the open.
    Pciefe(PciefeOptions),

    /// Only the M.2-to-`PCIe` breakout, with these options.
    Exm2pe(Exm2peOptions),

    /// Only XMASS (`fe=xmass`), whose bring-up is not ported.
    Xmass,

    /// Only `SuperSync` (`fe=supersync`), whose bring-up is not ported.
    SuperSync,

    /// Only `SimpleSync` (`fe=simplesync`), whose bring-up is not ported.
    SimpleSync,

    /// Only the 100–5000 MHz front end (`fe=fe1005000`), which libusdr admits on `LSDr`
    /// boards only: nothing is probed.
    Fe1005000,

    /// Only the four-channel 400–7200 MHz front end (`fe=fe4ch4007200`), which needs an
    /// I2C core the uSDR lacks: the open fails.
    Fe4ch4007200,
}

/// The front end a [`Device`](crate::Device) brought up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEndKind {
    /// The `PCIe` front end.
    Pciefe,

    /// The M.2-to-`PCIe` breakout; `dac` says whether its oscillator DAC answered.
    Exm2pe {
        /// The oscillator DAC identified itself.
        dac: bool,
    },
}

/// A front end that came up, with its driver state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontEnd {
    /// The `PCIe` front end.
    Pciefe(Pciefe),

    /// The M.2-to-`PCIe` breakout.
    Exm2pe(Exm2pe),
}

impl FrontEnd {
    /// What kind of board this is.
    pub(crate) fn kind(&self) -> FrontEndKind {
        match self {
            Self::Pciefe(_) => FrontEndKind::Pciefe,
            Self::Exm2pe(board) => FrontEndKind::Exm2pe {
                dac: board.has_dac(),
            },
        }
    }

    /// Sets the oscillator DAC (`dev_m2_lm6_1_sdr_vctcxo_set`): the `PCIe` front end's, or
    /// the breakout's if it answered.
    pub(crate) fn set_oscillator_dac(self, bus: &mut dyn Bus, code: u16) -> Result<(), Error> {
        match self {
            Self::Pciefe(_) => Pciefe::set_dac(bus, code),
            Self::Exm2pe(board) => board.set_dac(bus, code),
        }
    }
}

/// The boards libusdr tries, in its order (`enum fe_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Candidate {
    /// `FET_PCIE_DEVBOARD`.
    Pciefe,
    /// `FET_PCIE_XMASS`.
    Xmass,
    /// `FET_PCIE_SUPER_SYNC`.
    SuperSync,
    /// `FET_PCIE_SIMPLE_SYNC`.
    SimpleSync,
    /// `FET_PICE_BREAKOUT`.
    Exm2pe,
    /// `FET_PCIE_FE1005000`.
    Fe1005000,
    /// `FET_PCIE_FE4CH4007200`.
    Fe4ch4007200,
}

impl Candidate {
    /// Every board, in probe order.
    const ALL: [Self; 7] = [
        Self::Pciefe,
        Self::Xmass,
        Self::SuperSync,
        Self::SimpleSync,
        Self::Exm2pe,
        Self::Fe1005000,
        Self::Fe4ch4007200,
    ];

    /// Brings this board up; `Ok(None)` when it is absent or not admitted on the uSDR.
    /// `selection` carries a hint's options, if any.
    fn init(
        self,
        bus: &mut dyn Bus,
        selection: &FrontEndSelection,
    ) -> Result<Option<FrontEnd>, Error> {
        match self {
            Self::Pciefe => {
                let options = match selection {
                    FrontEndSelection::Pciefe(options) => Some(options),
                    _ => None,
                };
                Ok(Pciefe::init(bus, options)?.map(FrontEnd::Pciefe))
            }
            Self::Xmass => xmass::init(bus),
            Self::SuperSync => supersync::init(bus),
            Self::SimpleSync => simplesync::init(bus),
            Self::Exm2pe => {
                let options = match selection {
                    FrontEndSelection::Exm2pe(options) => Some(options),
                    _ => None,
                };
                Exm2pe::init(bus, options).map(|board| Some(FrontEnd::Exm2pe(board)))
            }
            // Admitted on LSDr (`lsdr`) only: `-ENODEV` before any bus access.
            Self::Fe1005000 => Ok(None),
            Self::Fe4ch4007200 => fe4ch::init(bus),
        }
    }

    /// The board a hint names, or `None` for [`FrontEndSelection::Auto`] and
    /// [`FrontEndSelection::None`].
    fn hinted(selection: &FrontEndSelection) -> Option<Self> {
        Some(match selection {
            FrontEndSelection::Auto | FrontEndSelection::None => return None,
            FrontEndSelection::Pciefe(_) => Self::Pciefe,
            FrontEndSelection::Exm2pe(_) => Self::Exm2pe,
            FrontEndSelection::Xmass => Self::Xmass,
            FrontEndSelection::SuperSync => Self::SuperSync,
            FrontEndSelection::SimpleSync => Self::SimpleSync,
            FrontEndSelection::Fe1005000 => Self::Fe1005000,
            FrontEndSelection::Fe4ch4007200 => Self::Fe4ch4007200,
        })
    }
}

/// Looks for a front end as `selection` says (`device_fe_probe` with compatibility `m2a+e`)
/// and brings up the first that answers.
///
/// A hinted board's failure fails the probe; while probing automatically, a failing board
/// is logged and skipped, as libusdr does. A board that answers but whose bring-up is not
/// ported fails the probe either way ([`Error::UnsupportedFrontEnd`]), where libusdr would
/// have brought it up.
pub(crate) fn probe(
    bus: &mut dyn Bus,
    selection: &FrontEndSelection,
) -> Result<Option<FrontEnd>, Error> {
    if *selection == FrontEndSelection::None {
        return Ok(None);
    }
    if let Some(candidate) = Candidate::hinted(selection) {
        return candidate.init(bus, selection);
    }
    for candidate in Candidate::ALL {
        match candidate.init(bus, selection) {
            Ok(Some(front_end)) => {
                debug!(kind = ?front_end.kind(), "front end detected");
                return Ok(Some(front_end));
            }
            Ok(None) => {}
            Err(error @ Error::UnsupportedFrontEnd(_)) => return Err(error),
            Err(error) => warn!(?candidate, %error, "front end failed to come up; skipped"),
        }
    }
    debug!("no front end detected");
    Ok(None)
}
