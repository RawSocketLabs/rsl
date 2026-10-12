//! Manual RX path selection: `dev_m2_lm6_1_sdr_rx_path_set` in
//! `device/m2_lm6_1/m2_lm6_1.c`, through `usdr_rfic_fe_set_rxlna` and
//! `usdr_rfic_switch_loopback` in `usdr_ctrl.c`.
//!
//! Tuning picks the RX input from the frequency ([`RxBand::for_frequency`]); a fixed path
//! overrides that until [`RxPath::Auto`] hands it back. The loopback variants also feed the
//! TX mixer's output into the chosen input, for self-test and calibration.

use super::Board;
use super::tune::RxBand;
use crate::error::Error;

/// Which RX input the receiver uses (libusdr's `/dm/sdr/0/rx/path`).
///
/// libusdr takes these as strings, case-insensitively: `rxl`, `rxw` and `rxh`, or with an
/// `_lb` suffix for loopback, and also its band names (`LNAL`/`EXT`, `LNAW`/`W`,
/// `LNAH`/`H`). Any other string selects [`RxPath::Auto`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxPath {
    /// Follow the frequency: the board mixer into LNA3 below 230 MHz, LNA1 to 2.8 GHz,
    /// LNA2 above. The power-up default.
    Auto,

    /// The low band, through the board's upconverting mixer into LNA3 (`rxl`).
    Low,

    /// The wideband input, LNA1 (`rxw`).
    Wide,

    /// The high band, LNA2 (`rxh`).
    High,

    /// [`Self::Low`], with the TX mixer's output looped back into it (`rxl_lb`).
    LowLoopback,

    /// [`Self::Wide`], with the TX mixer's output looped back into it (`rxw_lb`).
    WideLoopback,

    /// [`Self::High`], with the TX mixer's output looped back into it (`rxh_lb`).
    HighLoopback,
}

impl RxPath {
    /// The band this path fixes and whether it loops the transmitter back, or `None` for
    /// [`Self::Auto`].
    fn fixed(self) -> Option<(RxBand, bool)> {
        match self {
            Self::Auto => None,
            Self::Low => Some((RxBand::Mixer, false)),
            Self::Wide => Some((RxBand::Wide, false)),
            Self::High => Some((RxBand::High, false)),
            Self::LowLoopback => Some((RxBand::Mixer, true)),
            Self::WideLoopback => Some((RxBand::Wide, true)),
            Self::HighLoopback => Some((RxBand::High, true)),
        }
    }
}

impl Board {
    /// Selects the RX path (`usdr_rfic_fe_set_rxlna`), then switches the RF loopback.
    ///
    /// [`RxPath::Auto`] applies the band for the current frequency at once, without
    /// powering the receiver (`_usdr_signal_event(USDR_RX_LNA_CHANGED)`); before any
    /// frequency is set that is the mixer band, as in libusdr. A failed band selection
    /// leaves the loopback alone.
    pub(crate) fn set_rx_path(&mut self, path: RxPath) -> Result<(), Error> {
        let fixed = path.fixed();
        self.rx.band_fixed = fixed.is_some();
        let (band, loopback) = fixed.unwrap_or((RxBand::for_frequency(self.rx.lo_hz), false));
        self.select_rx_band(band)?;
        self.set_rf_loopback(loopback)
    }

    /// Turns the RF loopback on or off, if it is not already (`usdr_rfic_switch_loopback`).
    /// Like libusdr, the new state is recorded even if a write fails.
    fn set_rf_loopback(&mut self, on: bool) -> Result<(), Error> {
        if self.rx.rf_loopback == on {
            return Ok(());
        }
        let bus = self.bus.as_mut();
        let switched = if on {
            self.lms.enable_rf_loopback(bus)
        } else {
            self.lms.disable_rf_loopback(bus)
        };
        self.rx.rf_loopback = on;
        switched
    }
}
