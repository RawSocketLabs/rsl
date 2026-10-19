//! The RX and TX paths and their RF loopback (`lms7002m_rfe_set_path`,
//! `lms7002m_tfe_set_path`, `lms7002m_fe_set_lna`) and the baseband NCOs
//! (`lms7002m_bb_set_freq`).

use super::calls::{Calls, ChipCall};
use super::op::{Band, Piece, Pieces};
use super::{Control, ControlError, Direction, Frontend, NoFrontend, Operation, RfPath};
use super::{RxRoute, TxAntenna, TxRoute};
use crate::chip::{Tsp, TxPath};
use crate::regs::{Mac, RxPath};

/// 2³²: the NCO's frequency word for the converter clock.
const NCO_SCALE: f64 = 4_294_967_296.0;

impl RxRoute {
    /// The LNA and whether the RF loopback feeds it; `None` for [`Self::Auto`].
    const fn lna(self) -> Option<(RxPath, bool)> {
        match self {
            Self::Auto => None,
            Self::LnaL => Some((RxPath::LnaL, false)),
            Self::LnaH => Some((RxPath::LnaH, false)),
            Self::LnaW => Some((RxPath::LnaW, false)),
            Self::LoopbackL => Some((RxPath::LnaL, true)),
            Self::LoopbackW => Some((RxPath::LnaW, true)),
            Self::LoopbackH => Some((RxPath::LnaH, true)),
        }
    }
}

impl Control {
    /// Selects the receive path (`lms7002m_rfe_set_path`): the band whose LNA it names, or
    /// with [`RxRoute::Auto`] the band the RX LO falls in, its front end set and the
    /// board's switch moved; the RF loopback paths also set the TX band feeding the LNA
    /// in loopback and use the band's loopback switch position. Turning the loopback on
    /// sets the pad's loopback loss to 0 dB; turning it off restores the pad's loss and
    /// the TX band's front end and switch. Either way both channels' receive gains are
    /// then rewritten.
    ///
    /// The writes reach the selected banks, as in libusdr, which selects none here.
    ///
    /// # Errors
    ///
    /// [`ControlError::InvalidPath`] when the board has no band for the LNA, after the path
    /// and loopback are stored and before any write; [`ControlError::NoChannel`] with no
    /// bank selected.
    #[must_use]
    pub fn set_rx_path<F: Frontend>(self, frontend: F, route: RxRoute) -> Operation<F> {
        let mut pieces = Pieces::new();
        pieces.push(Piece::RxPath(route));
        Operation::script(&self, frontend, pieces)
    }

    /// Selects the transmit path (`lms7002m_tfe_set_path`): the band whose output, or
    /// whose name (`W`, `H`), it names, its front end set and the board's switch moved, in
    /// RF loopback as the receive path left it. [`TxRoute::Auto`] turns the loopback off
    /// (without restoring anything) and selects the band the TX LO falls in.
    ///
    /// # Errors
    ///
    /// [`ControlError::InvalidPath`] when the board has no such band, before any change;
    /// [`ControlError::NoChannel`] with no bank selected.
    #[must_use]
    pub fn set_tx_path<F: Frontend>(self, frontend: F, route: TxRoute) -> Operation<F> {
        let mut pieces = Pieces::new();
        pieces.push(Piece::TxPath(route));
        Operation::script(&self, frontend, pieces)
    }

    /// Selects a path on `channels` (`lms7002m_fe_set_lna`): the banks selected, then
    /// [`Self::set_tx_path`] for the TX band and automatic paths and [`Self::set_rx_path`]
    /// for the RX ones. The `W` and `H` TX paths reach the RX selection, which refuses
    /// them, as in libusdr.
    ///
    /// # Errors
    ///
    /// As those, and [`ControlError::InvalidPath`] for [`TxRoute::W`] and [`TxRoute::H`]
    /// after the bank selection.
    #[must_use]
    pub fn set_rf_path<F: Frontend>(
        self,
        frontend: F,
        channels: Mac,
        path: RfPath,
    ) -> Operation<F> {
        let mut pieces = Pieces::new();
        pieces.push(Piece::MacSet(channels));
        match path {
            RfPath::Tx(route @ (TxRoute::Band1 | TxRoute::Band2 | TxRoute::Auto)) => {
                pieces.push(Piece::TxPath(route));
            }
            RfPath::Tx(TxRoute::W | TxRoute::H) => {
                pieces.push(Piece::Refuse(ControlError::InvalidPath));
            }
            RfPath::Rx(route) => {
                pieces.push(Piece::RxPath(route));
            }
        }
        Operation::script(&self, frontend, pieces)
    }

    /// `lms7002m_rfe_set_path`'s state changes and steps, made when its piece is reached.
    pub(super) fn rx_path_pieces(&mut self, route: RxRoute, pieces: &mut Pieces) {
        let lna = route.lna();
        let loopback = lna.is_some_and(|(_, loopback)| loopback);
        let changed = self.loopback != loopback;
        self.loopback = loopback;
        self.rx_route = route;
        match lna {
            None => {
                pieces.push(Piece::LnaRx(Band::Signal));
            }
            Some((path, _)) => {
                let Some(band) = self.rx_bands.iter().position(|band| band.path == path) else {
                    pieces.push(Piece::Refuse(ControlError::InvalidPath));
                    return;
                };
                self.rx_band = band;
                pieces.push(Piece::LnaRx(Band::Stored));
            }
        }
        pieces.push(Piece::Switch);
        if changed {
            if loopback {
                pieces.push(Piece::LoopbackOn);
            } else {
                pieces
                    .push(Piece::PadRestore)
                    .push(Piece::LnaTx(Band::Stored))
                    .push(Piece::Switch);
            }
            pieces.push(Piece::RfeGains);
        }
    }

    /// `lms7002m_tfe_set_path`'s state changes and steps, made when its piece is reached.
    pub(super) fn tx_path_pieces(&mut self, route: TxRoute, pieces: &mut Pieces) {
        let band = match route {
            TxRoute::Auto => {
                self.tx_route = TxRoute::Auto;
                self.loopback = false;
                pieces.push(Piece::LnaTx(Band::Signal)).push(Piece::Switch);
                return;
            }
            TxRoute::Band1 => self.tx_band_with(|band| band.path == TxPath::Band1),
            TxRoute::Band2 => self.tx_band_with(|band| band.path == TxPath::Band2),
            TxRoute::W => self.tx_band_named("W"),
            TxRoute::H => self.tx_band_named("H"),
        };
        let Some(band) = band else {
            pieces.push(Piece::Refuse(ControlError::InvalidPath));
            return;
        };
        self.tx_route = route;
        self.tx_band = band;
        pieces.push(Piece::LnaTx(Band::Stored)).push(Piece::Switch);
    }

    /// The first TX band `matches` accepts (`get_antenna_cfg_by_band`).
    fn tx_band_with(&self, matches: impl Fn(&TxAntenna) -> bool) -> Option<usize> {
        self.tx_bands.iter().position(matches)
    }

    /// The first TX band named `name` (`get_antenna_cfg_by_name`).
    fn tx_band_named(&self, name: &str) -> Option<usize> {
        self.tx_band_with(|band| band.names.iter().any(|n| n.eq_ignore_ascii_case(name)))
    }

    /// Sets a baseband NCO to `hertz` on `channels` (`lms7002m_bb_set_freq`): the
    /// frequency as a 32-bit fraction of the converter clock (CGEN over the ADC or DAC
    /// divider), stored per channel; when the direction runs, the banks selected and the
    /// processor's mixer set, from channel B's word for B alone, else A's. Streaming up
    /// restores it.
    ///
    /// A frequency of exactly half the converter clock gives the word 2³¹, which libusdr
    /// converts to `INT32_MIN` (the same frequency mirrored): reproduced.
    ///
    /// # Errors
    ///
    /// Before any write: [`ControlError::NoChannel`] for no channel;
    /// [`ControlError::NoRate`] before a sample rate set the converter clock (libusdr
    /// divides by zero); [`ControlError::NcoOutOfRange`] beyond half the converter clock.
    #[must_use]
    pub fn set_nco(mut self, direction: Direction, channels: Mac, hertz: i32) -> Operation {
        let frontend = NoFrontend(());
        if channels == Mac::None {
            return Operation::finished(&self, frontend, Err(ControlError::NoChannel));
        }
        let divider = match direction {
            Direction::Rx => self.clocks.rx_converter_div,
            Direction::Tx => self.clocks.tx_converter_div,
        };
        let Some(converter) = self
            .clocks
            .cgen
            .checked_div(divider)
            .filter(|&clock| clock != 0)
        else {
            return Operation::finished(&self, frontend, Err(ControlError::NoRate));
        };
        let relative = f64::from(hertz) / f64::from(converter);
        if !(-0.5..=0.5).contains(&relative) {
            return Operation::finished(&self, frontend, Err(ControlError::NcoOutOfRange));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr's double-to-int32 conversion: within ±2^31, 2^31 wrapping"
        )]
        let word = (relative * NCO_SCALE) as i64 as i32;
        let (words, running, tsp) = match direction {
            Direction::Rx => (&mut self.rx_nco, self.rx_running, Tsp::Rx),
            Direction::Tx => (&mut self.tx_nco, self.tx_running, Tsp::Tx),
        };
        if matches!(channels, Mac::A | Mac::Both) {
            words[0] = Some(word);
        }
        if matches!(channels, Mac::B | Mac::Both) {
            words[1] = Some(word);
        }
        let mut calls = Calls::new();
        if running.contains(&true) {
            let index = usize::from(channels == Mac::B);
            calls
                .push(ChipCall::MacSet(channels))
                .push(ChipCall::TspCmix(tsp, words[index].unwrap_or(0)));
        }
        Operation::writes(&self, &calls)
    }
}
