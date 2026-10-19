//! What to probe for: libusdr's `fe=` hint.

use crate::board::Board;
use crate::exm2pe::Exm2peOptions;
use crate::pciefe::PciefeOptions;

/// Which front end to look for: libusdr's `fe=` option, typed.
///
/// A hint names one board; libusdr matches the option's start against each board's name
/// (`fe=xmass:anything` names XMASS), and only the `PCIe` front end and the breakout read
/// what follows. A hint for a board the device's connector does not admit finds nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrontEndSelection {
    /// Try every board the connector admits in libusdr's order and keep the first that
    /// comes up; a board that fails is skipped. libusdr's default (no `fe=`).
    #[default]
    Auto,

    /// Probe nothing. libusdr does this for an `fe=` naming no board (`fe=none`).
    None,

    /// Only the `PCIe` front end, with these options; a failure fails the probe.
    Pciefe(PciefeOptions),

    /// Only the M.2-to-`PCIe` breakout, with these options.
    Exm2pe(Exm2peOptions),

    /// Only XMASS (`fe=xmass`).
    Xmass,

    /// Only `SuperSync` (`fe=supersync`).
    SuperSync,

    /// Only `SimpleSync` (`fe=simplesync`).
    SimpleSync,

    /// Only the 100–5000 MHz front end (`fe=fe1005000`).
    Fe1005000,

    /// Only the four-channel 400–7200 MHz front end (`fe=fe4ch4007200`).
    Fe4ch4007200,
}

impl FrontEndSelection {
    /// The board a hint names, or `None` for [`Self::Auto`] and [`Self::None`].
    #[must_use]
    pub const fn hinted(&self) -> Option<Board> {
        Some(match self {
            Self::Auto | Self::None => return None,
            Self::Pciefe(_) => Board::Pciefe,
            Self::Exm2pe(_) => Board::Exm2pe,
            Self::Xmass => Board::Xmass,
            Self::SuperSync => Board::SuperSync,
            Self::SimpleSync => Board::SimpleSync,
            Self::Fe1005000 => Board::Fe1005000,
            Self::Fe4ch4007200 => Board::Fe4ch4007200,
        })
    }
}
