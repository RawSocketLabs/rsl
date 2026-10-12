//! Manual path selection: `dev_m2_lm6_1_sdr_rx_path_set` and `...tx_path_set` in
//! `device/m2_lm6_1/m2_lm6_1.c`, through `usdr_rfic_fe_set_rxlna`,
//! `usdr_rfic_switch_loopback` and `usdr_rfic_fe_set_txlna` in `usdr_ctrl.c`.
//!
//! Tuning picks the RX input and the TX output from the frequency
//! ([`RxBand::for_frequency`], [`TxBand::for_frequency`]); a fixed path overrides that until
//! [`RxPath::Auto`] or [`TxPath::Auto`] hands it back. The RX loopback variants also feed
//! the TX mixer's output into the chosen input, for self-test and calibration.

use super::Board;
use super::board::TxState;
use super::tune::{RxBand, TxBand};
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

/// Which TX output the transmitter uses (libusdr's `/dm/sdr/0/tx/path`).
///
/// libusdr takes these as strings, case-insensitively: `txw` and `txh`, and also its band
/// names (`W`/`B1`, `H`/`B2`). Any other string selects [`TxPath::Auto`]; a number makes it
/// read an arbitrary address as a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxPath {
    /// Follow the frequency: PA1 below 2.8 GHz, PA2 from there up. The power-up default.
    Auto,

    /// PA1, through the B0322J5050AHF balun (`txw`).
    Wide,

    /// PA2, through the 3600BL14M050 balun (`txh`).
    High,
}

impl TxPath {
    /// The band this path fixes, or `None` for [`Self::Auto`].
    fn fixed(self) -> Option<TxBand> {
        match self {
            Self::Auto => None,
            Self::Wide => Some(TxBand::Wide),
            Self::High => Some(TxBand::High),
        }
    }
}

impl Board {
    /// Selects the TX path (`usdr_rfic_fe_set_txlna`), with `tx` the transmitter's
    /// settings. [`TxPath::Auto`] applies the band for the current TX frequency at once,
    /// without powering the transmitter (`_usdr_signal_event(USDR_TX_LNA_CHANGED)`); before
    /// any frequency is set that is PA1, as in libusdr. Neither retunes.
    pub(crate) fn set_tx_path(&mut self, tx: &mut TxState, path: TxPath) -> Result<(), Error> {
        let fixed = path.fixed();
        tx.band_fixed = fixed.is_some();
        self.select_tx_band(fixed.unwrap_or_else(|| TxBand::for_frequency(tx.lo_hz)))
    }

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
