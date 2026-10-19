//! The control code's operations as one hand-written machine: each runs its parts (chip
//! calls, tunings, board switches) in libusdr's order, holding the [`Control`] state.

use bnb::u3;
use core::num::NonZeroU32;

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::calls::{Calls, ChipCall};
use super::part::Part;
use super::rate::{Candidate, RateRequest};
use super::{
    Control, Direction, Frontend, NoFrontend, RxRoute, StreamFlags, TxRoute, band_for, order,
    push_bandwidth,
};
use crate::chip::{
    LmlConfig, RbbMode, RbbPath, RxMode, Synthesizer, TbbMode, TbbPath, Tsp, TuneError,
    TxGainStage, TxMode, TxPath,
};
use crate::regs::{Mac, RxPath};

/// The RX DC corrector's window `lms7002m_streaming_up` sets.
const DC_WINDOW: u8 = 7;
/// CGEN tunings per candidate before libusdr tries the next.
const CGEN_ATTEMPTS: u8 = 4;
/// The most pieces one path operation runs.
const PIECES: usize = 12;

/// Why a control operation failed, beyond a bus failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// No channel selected for a banked call, or none asked for: libusdr's `-EINVAL`.
    NoChannel,

    /// A processor cannot divide down to the rate asked: `lms7002m_samplerate`'s
    /// `-EINVAL` ("can't deliver decimation").
    Decimation,

    /// CGEN did not lock on any candidate: `lms7002m_samplerate`'s `-ERANGE` ("can't tune
    /// VCO for data clock").
    Clock,

    /// An LO synthesizer did not tune.
    Tune(TuneError),

    /// Streaming up or an NCO before any sample rate, where libusdr divides by zero;
    /// refused before any write.
    NoRate,

    /// A rate request libusdr would hang or divide by zero on: no rate in either
    /// direction with no converter floor (CGEN 0), a zero board decimation or
    /// interpolation, or one larger than its rate. Refused before any write.
    InvalidRate,

    /// A path the board's band table has no band for, or a TX path where an RX one is
    /// taken: libusdr's `-EINVAL`.
    InvalidPath,

    /// An NCO frequency beyond half the converter clock: `lms7002m_bb_translate`'s
    /// `-EINVAL` ("NCO ... out of range").
    NcoOutOfRange,
}

/// Which band a front-end piece selects: libusdr's `_lms7002m_set_lna_rx/_tx` with the
/// stored band, or reached through `_lms7002m_signal_event`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Band {
    /// A signal event (`XSDR_*_LNA_CHANGED`, `XSDR_*_LO_CHANGED`): an automatic path's
    /// band follows the LO; an explicit path is left alone.
    Signal,

    /// Streaming up: an automatic path's band follows the LO, an explicit one keeps its
    /// band (`rfic_path == AUTO ? signal : set_lna(cfg_path)`).
    Current,

    /// The stored band (`_lms7002m_set_lna_*(d, d->*_cfg_path)`).
    Stored,
}

/// One step of a path operation, expanded into its part only when reached, from the state
/// earlier steps left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Piece {
    /// `lms7002m_mac_set`.
    MacSet(Mac),

    /// The receive band's front end (`_lms7002m_set_lna_rx` up to its switch): the LNA,
    /// and in RF loopback the TX band feeding it.
    LnaRx(Band),

    /// The transmit band's front end (`_lms7002m_set_lna_tx` up to its switch).
    LnaTx(Band),

    /// The antenna switch the last front-end piece chose, if any.
    Switch,

    /// The RF loopback turned on (`_lms7002m_lb_status_changes`): the pad's loopback loss
    /// at 0 dB.
    LoopbackOn,

    /// The RF loopback turned off: the pad's stored loss restored.
    PadRestore,

    /// Both channels' receive gains rewritten (`lms7002m_rfe_gain(.., RFE_GAIN_NONE, ..)`).
    RfeGains,

    /// `lms7002m_rfe_set_path`: its state changes made when reached, after the pieces
    /// before it, then its own pieces run. Last in its list.
    RxPath(RxRoute),

    /// `lms7002m_tfe_set_path`, as [`Self::RxPath`].
    TxPath(TxRoute),

    /// The operation fails here.
    Refuse(ControlError),
}

/// A fixed list of pieces.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pieces {
    /// The pieces; the first `len` are used.
    list: [Piece; PIECES],

    /// How many it holds.
    len: usize,
}

impl Pieces {
    /// No pieces.
    pub(super) const fn new() -> Self {
        Self {
            list: [Piece::Switch; PIECES],
            len: 0,
        }
    }

    /// Appends a piece.
    ///
    /// # Panics
    ///
    /// Past the capacity: every list the control code builds is bounded.
    pub(super) fn push(&mut self, piece: Piece) -> &mut Self {
        assert!(
            self.len < PIECES,
            "invariant: piece lists fit their capacity"
        );
        self.list[self.len] = piece;
        self.len += 1;
        self
    }
}

/// Where an operation is.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// Running its last part.
    Last,

    /// Writing a sample rate's dividers and `LimeLight` configuration, which is cached
    /// once written.
    RateWrites {
        /// The configuration.
        lml: LmlConfig,
    },

    /// Tuning CGEN for a sample rate: `pass` over the CGEN ceilings, `attempt` at this
    /// candidate.
    Rate {
        /// The request.
        request: RateRequest,
        /// The ceiling pass.
        pass: u8,
        /// The tuning attempt.
        attempt: u8,
        /// The candidate being tuned.
        candidate: Candidate,
    },

    /// Streaming up: the map, AFE, and RX processor and baseband writes.
    UpPrepare {
        /// The receive direction.
        rx: Up,
        /// The transmit direction, when it streams up too.
        tx: Option<Up>,
    },

    /// Streaming up RX: the band's front end selected; its switch follows.
    UpBand {
        /// The receive direction.
        rx: Up,
        /// The transmit direction, when it streams up too.
        tx: Option<Up>,
    },

    /// Streaming up RX: the antenna switch.
    UpSwitch {
        /// The receive direction.
        rx: Up,
        /// The transmit direction, when it streams up too.
        tx: Option<Up>,
    },

    /// Streaming up RX: each channel's bandwidth and mixer restored; TX follows.
    UpRestore {
        /// The transmit direction.
        tx: Up,
    },

    /// Streaming up TX: the processor, baseband and front-end writes, and each channel's
    /// bandwidth and mixer restored.
    UpTxPrepare {
        /// The transmit direction.
        tx: Up,
    },

    /// Streaming up TX: the banks reselected and the band's front end selected; its
    /// switch follows.
    UpTxBand,

    /// Stopping a synthesizer, whose write failures libusdr ignores.
    TuneOff,

    /// TDD tuning: the RX synthesizer stopped first, its write failures ignored.
    TddOff {
        /// The LO asked.
        hertz: u32,
    },

    /// Locking a synthesizer.
    TuneLock {
        /// The synthesizer.
        synthesizer: Synthesizer,
        /// The LO asked.
        hertz: u32,
        /// TDD: both LOs follow the TX synthesizer.
        tdd: bool,
    },

    /// Running pieces in order.
    Script {
        /// The pieces.
        pieces: Pieces,
        /// The next piece.
        at: usize,
    },

    /// Finished.
    Done(Result<(), ControlError>),
}

/// One direction of a streaming up, decided before any write.
#[derive(Clone, Copy, Debug)]
struct Up {
    /// The banks streamed (`rx_chs`, `tx_chs`), SISO's correction applied.
    channels: Mac,
    /// The channels that will run (`rxafen_*`, `txafen_*`).
    running: [bool; 2],
    /// Each streamed channel's bandwidth: the one set, or the sample rate's.
    bandwidths: [Option<NonZeroU32>; 2],
}

/// A running control operation. It owns the [`Control`] state until it finishes; take it
/// back with [`Self::control`], also after a failure (libusdr keeps what it updated).
#[derive(Clone, Copy, Debug)]
pub struct Operation<F: Frontend = NoFrontend> {
    /// The state.
    control: Control,

    /// The board's hooks.
    frontend: F,

    /// Where the operation is.
    stage: Stage,

    /// The part running.
    part: Part<F::Switch>,

    /// The antenna switch the last front-end selection chose, not yet moved.
    switch: Option<(Direction, u8)>,

    /// A bus access failed.
    failed: bool,
}

impl Operation<NoFrontend> {
    /// Runs `calls` and finishes.
    pub(super) fn writes(control: &Control, calls: &Calls) -> Self {
        Self::start(*control, NoFrontend(()), Stage::Last, calls)
    }
}

impl<F: Frontend> Operation<F> {
    /// An operation at `stage` running `calls` first.
    fn start(mut control: Control, frontend: F, stage: Stage, calls: &Calls) -> Self {
        let part = Part::calls(*calls, &mut control.chip);
        let mut operation = Self {
            control,
            frontend,
            stage,
            part,
            switch: None,
            failed: false,
        };
        operation.settle();
        operation
    }

    /// An operation that finished before any access.
    pub(super) fn finished(
        control: &Control,
        frontend: F,
        outcome: Result<(), ControlError>,
    ) -> Self {
        Self::start(*control, frontend, Stage::Done(outcome), &Calls::new())
    }

    /// An operation running `pieces` in order.
    pub(super) fn script(control: &Control, frontend: F, pieces: Pieces) -> Self {
        let mut operation = Self::finished(control, frontend, Ok(()));
        operation.stage = Stage::Script { pieces, at: 0 };
        operation.settle();
        operation
    }

    /// The control state, as the operation has left it so far.
    #[must_use]
    pub const fn control(&self) -> &Control {
        &self.control
    }

    /// The outcome once finished; a bus failure is reported by [`Sequence::complete`]
    /// instead and leaves this `None`.
    #[must_use]
    pub const fn outcome(&self) -> Option<Result<(), ControlError>> {
        match self.stage {
            Stage::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// Advances past finished parts until one has an action or the operation is done.
    fn settle(&mut self) {
        while self.outcome().is_none() && self.part.next().is_none() {
            if matches!(self.part, Part::Calls(calls) if calls.refused()) {
                self.stage = Stage::Done(Err(ControlError::NoChannel));
                return;
            }
            self.advance();
        }
    }

    /// Runs `calls` next.
    fn calls(&mut self, calls: &Calls) {
        self.part = Part::calls(*calls, &mut self.control.chip);
    }

    /// The stage after the current part finished.
    fn advance(&mut self) {
        let next = match self.stage {
            Stage::Last | Stage::TuneOff => Stage::Done(Ok(())),
            Stage::RateWrites { lml } => {
                self.control.lml = lml;
                Stage::Done(Ok(()))
            }
            Stage::Rate {
                request,
                pass,
                attempt,
                candidate,
            } => self.after_cgen(request, pass, attempt, candidate),
            Stage::UpPrepare { rx, tx } => {
                // libusdr sets the run flags before selecting the band.
                self.control.rx_running = rx.running;
                let mut calls = Calls::new();
                self.lna_rx(Band::Current, &mut calls);
                self.calls(&calls);
                Stage::UpBand { rx, tx }
            }
            Stage::UpBand { rx, tx } => {
                if let Some((direction, position)) = self.switch.take() {
                    self.part = Part::Board(self.frontend.antenna_switch(direction, position));
                }
                Stage::UpSwitch { rx, tx }
            }
            Stage::UpSwitch { rx, tx } => {
                let mut calls = Calls::new();
                push_restore(&mut calls, Direction::Rx, rx, self.control.rx_nco);
                self.calls(&calls);
                tx.map_or(Stage::Last, |tx| Stage::UpRestore { tx })
            }
            Stage::UpRestore { tx } => {
                let mut calls = Calls::new();
                calls
                    .push(ChipCall::MacSet(tx.channels))
                    .push(ChipCall::TspIntDec(
                        Tsp::Tx,
                        order(self.control.clocks.tx_tsp_div),
                    ))
                    .push(ChipCall::TspEnable(Tsp::Tx, true))
                    .push(ChipCall::TbbPath(TbbPath::Ladder, TbbMode::Normal))
                    // The band follows below.
                    .push(ChipCall::TrfPath(TxPath::Mute, TxMode::Normal));
                push_restore(&mut calls, Direction::Tx, tx, self.control.tx_nco);
                self.calls(&calls);
                Stage::UpTxPrepare { tx }
            }
            Stage::UpTxPrepare { tx } => {
                // libusdr sets the run flags before reselecting the banks.
                self.control.tx_running = tx.running;
                let mut calls = Calls::new();
                calls.push(ChipCall::MacSet(tx.channels));
                self.lna_tx(Band::Current, &mut calls);
                self.calls(&calls);
                Stage::UpTxBand
            }
            Stage::UpTxBand => {
                if let Some((direction, position)) = self.switch.take() {
                    self.part = Part::Board(self.frontend.antenna_switch(direction, position));
                }
                Stage::Last
            }
            Stage::TddOff { hertz } => self.lock(Synthesizer::Tx, hertz, true),
            Stage::TuneLock {
                synthesizer,
                hertz,
                tdd,
            } => self.after_lock(synthesizer, hertz, tdd),
            Stage::Script { pieces, at } => self.next_piece(pieces, at),
            Stage::Done(outcome) => Stage::Done(outcome),
        };
        self.stage = next;
    }

    /// Expands the piece at `at` into the part to run, or finishes after the last.
    fn next_piece(&mut self, pieces: Pieces, at: usize) -> Stage {
        let Some(&piece) = pieces.list[..pieces.len].get(at) else {
            return Stage::Done(Ok(()));
        };
        let mut calls = Calls::new();
        match piece {
            Piece::MacSet(mac) => {
                calls.push(ChipCall::MacSet(mac));
            }
            Piece::LnaRx(band) => self.lna_rx(band, &mut calls),
            Piece::LnaTx(band) => self.lna_tx(band, &mut calls),
            Piece::Switch => {
                if let Some((direction, position)) = self.switch.take() {
                    self.part = Part::Board(self.frontend.antenna_switch(direction, position));
                    return Stage::Script { pieces, at: at + 1 };
                }
            }
            Piece::LoopbackOn => {
                calls.push(ChipCall::TrfGain(TxGainStage::Loopback, 0));
            }
            Piece::PadRestore => {
                // `-10 * tx_loss[]`, channel A's when A is selected (`reg_mac & 1`), else B's.
                let control = &self.control;
                let a = matches!(control.chip.mac(), Mac::A | Mac::Both);
                let loss = control.tx_loss[usize::from(!a)];
                calls.push(ChipCall::TrfGain(TxGainStage::Pad, -10 * i32::from(loss)));
            }
            Piece::RfeGains => {
                calls.push(ChipCall::RfeGains);
            }
            Piece::RxPath(route) => {
                let mut expanded = Pieces::new();
                self.control.rx_path_pieces(route, &mut expanded);
                return self.next_piece(expanded, 0);
            }
            Piece::TxPath(route) => {
                let mut expanded = Pieces::new();
                self.control.tx_path_pieces(route, &mut expanded);
                return self.next_piece(expanded, 0);
            }
            Piece::Refuse(error) => return Stage::Done(Err(error)),
        }
        self.calls(&calls);
        Stage::Script { pieces, at: at + 1 }
    }

    /// `_lms7002m_set_lna_rx`'s writes for the band `band` picks, its switch kept for a
    /// [`Piece::Switch`], pushed onto `calls`; none when a signal leaves an explicit path
    /// alone.
    fn lna_rx(&mut self, band: Band, calls: &mut Calls) {
        let control = &mut self.control;
        // The stored band is used as it is (`_lms7002m_set_lna_*(d, d->*_cfg_path)`); a
        // signal or streaming up recomputes an automatic path's.
        if band != Band::Stored {
            if control.rx_route == RxRoute::Auto {
                control.rx_band = band_for(
                    control.rx_lo,
                    control.rx_bands.iter().map(|band| band.below_hz),
                );
            } else if band == Band::Signal {
                return;
            }
        }
        let antenna = control.rx_bands[control.rx_band];
        if control.loopback {
            calls
                .push(ChipCall::RfePath(antenna.path, RxMode::LoopbackRf))
                .push(ChipCall::TrfPath(
                    TxPath::looped_from(antenna.path),
                    TxMode::Loopback,
                ));
            self.switch = Some((Direction::Rx, antenna.loopback_switch));
        } else {
            calls.push(ChipCall::RfePath(antenna.path, RxMode::Normal));
            self.switch = Some((Direction::Rx, antenna.switch));
        }
    }

    /// `_lms7002m_set_lna_tx`'s writes, as [`Self::lna_rx`].
    fn lna_tx(&mut self, band: Band, calls: &mut Calls) {
        let control = &mut self.control;
        // The stored band is used as it is (`_lms7002m_set_lna_*(d, d->*_cfg_path)`); a
        // signal or streaming up recomputes an automatic path's.
        if band != Band::Stored {
            if control.tx_route == TxRoute::Auto {
                control.tx_band = band_for(
                    control.tx_lo,
                    control.tx_bands.iter().map(|band| band.below_hz),
                );
            } else if band == Band::Signal {
                return;
            }
        }
        let antenna = control.tx_bands[control.tx_band];
        if control.loopback {
            let lna = if antenna.path == TxPath::Band1 {
                RxPath::LnaW
            } else {
                RxPath::LnaL
            };
            calls
                .push(ChipCall::RfePath(lna, RxMode::LoopbackRf))
                .push(ChipCall::TrfPath(antenna.path, TxMode::Loopback));
            self.switch = Some((Direction::Tx, antenna.loopback_switch));
        } else {
            calls.push(ChipCall::TrfPath(antenna.path, TxMode::Normal));
            self.switch = Some((Direction::Tx, antenna.switch));
        }
    }

    /// The stage after a CGEN tuning finished: the rate's writes once it locked, else the
    /// next attempt, the next candidate or the failure.
    fn after_cgen(
        &mut self,
        request: RateRequest,
        pass: u8,
        attempt: u8,
        candidate: Candidate,
    ) -> Stage {
        let Part::Cgen(tune) = self.part else {
            return Stage::Done(Err(ControlError::Clock));
        };
        if tune.outcome() == Some(Ok(())) {
            self.control.clocks.cgen = candidate.cgen;
            let (calls, lml) = self.control.rate_writes(&request, candidate);
            self.calls(&calls);
            return Stage::RateWrites { lml };
        }
        if attempt + 1 < CGEN_ATTEMPTS {
            return self.tune_cgen(request, pass, attempt + 1, candidate);
        }
        if pass + 1 < request.passes() {
            return match request.candidate(pass + 1) {
                Ok(next) => self.tune_cgen(request, pass + 1, 0, next),
                Err(error) => Stage::Done(Err(error)),
            };
        }
        Stage::Done(Err(ControlError::Clock))
    }

    /// Stores `candidate`'s clocks (libusdr does before tuning) and tunes CGEN for it.
    fn tune_cgen(
        &mut self,
        request: RateRequest,
        pass: u8,
        attempt: u8,
        candidate: Candidate,
    ) -> Stage {
        self.control.clocks = request.clocks(candidate, self.control.clocks.cgen);
        self.part = Part::Cgen(self.control.chip.cgen_tune(
            self.control.reference,
            candidate.cgen,
            RateRequest::dac_order(candidate),
        ));
        Stage::Rate {
            request,
            pass,
            attempt,
            candidate,
        }
    }

    /// Locks `synthesizer` on `hertz`; `tdd` for the TX synthesizer serving both.
    fn lock(&mut self, synthesizer: Synthesizer, hertz: u32, tdd: bool) -> Stage {
        let tune = self
            .control
            .chip
            .sxx_tune(synthesizer, self.control.reference, hertz, tdd);
        self.part = Part::Sxx(tune);
        Stage::TuneLock {
            synthesizer,
            hertz,
            tdd,
        }
    }

    /// The stage after an LO synthesizer's tuning finished: on a lock, the LOs stored and,
    /// for each running path the LO serves, libusdr's `XSDR_*_LO_CHANGED` signal: both
    /// banks selected and an automatic path's band following the new LO.
    fn after_lock(&mut self, synthesizer: Synthesizer, hertz: u32, tdd: bool) -> Stage {
        let Part::Sxx(tune) = self.part else {
            return Stage::Done(Err(ControlError::Tune(TuneError::OutOfRange)));
        };
        match tune.outcome() {
            Some(Ok(())) => {}
            Some(Err(error)) => return Stage::Done(Err(ControlError::Tune(error))),
            None => return Stage::Done(Err(ControlError::Tune(TuneError::OutOfRange))),
        }
        let control = &mut self.control;
        let (rx, tx) = match (synthesizer, tdd) {
            (_, true) => (true, true),
            (Synthesizer::Rx, false) => (true, false),
            (Synthesizer::Tx, false) => (false, true),
        };
        if rx {
            control.rx_lo = hertz;
        }
        if tx {
            control.tx_lo = hertz;
        }
        let mut pieces = Pieces::new();
        if rx && control.rx_running.contains(&true) {
            pieces
                .push(Piece::MacSet(Mac::Both))
                .push(Piece::LnaRx(Band::Signal))
                .push(Piece::Switch);
        }
        if tx && control.tx_running.contains(&true) {
            pieces
                .push(Piece::MacSet(Mac::Both))
                .push(Piece::LnaTx(Band::Signal))
                .push(Piece::Switch);
        }
        self.next_piece(pieces, 0)
    }
}

/// `lms7002m_streaming_up`'s restore loop for one direction: per streamed channel, its
/// bank, its bandwidth (set, or the sample rate's) and its mixer at its NCO word
/// (`rx_dsp`, `tx_dsp`), or bypassed.
fn push_restore(calls: &mut Calls, direction: Direction, up: Up, nco: [Option<i32>; 2]) {
    let tsp = match direction {
        Direction::Rx => Tsp::Rx,
        Direction::Tx => Tsp::Tx,
    };
    for ((bank, bandwidth), nco) in [Mac::A, Mac::B].into_iter().zip(up.bandwidths).zip(nco) {
        // Only streamed channels have one.
        let Some(bandwidth) = bandwidth else {
            continue;
        };
        calls.push(ChipCall::MacSet(bank));
        push_bandwidth(calls, direction, bandwidth);
        calls.push(ChipCall::TspCmix(tsp, nco.unwrap_or(0)));
    }
}

impl Control {
    /// Sets the sample rate (`lms7002m_samplerate`): CGEN candidates tried in turn, four
    /// tunings each, then the processors' dividers, the `LimeLight` configuration and the
    /// running channels' bandwidths.
    ///
    /// libusdr retries a CGEN tuning that failed on the bus too; here a bus failure ends
    /// the operation. Requests libusdr would hang or divide by zero on are
    /// [`ControlError::InvalidRate`] before any write.
    #[must_use]
    pub fn sample_rate(self, request: RateRequest) -> Operation {
        let frontend = NoFrontend(());
        if !request.sound() {
            return Operation::finished(&self, frontend, Err(ControlError::InvalidRate));
        }
        let candidate = match request.candidate(0) {
            Ok(candidate) => candidate,
            Err(error) => return Operation::finished(&self, frontend, Err(error)),
        };
        let mut operation = Operation::finished(&self, frontend, Ok(()));
        operation.stage = operation.tune_cgen(request, 0, 0, candidate);
        operation.settle();
        operation
    }

    /// Streams `channels` up in the receive direction (the RX half of
    /// `lms7002m_streaming_up`, with the stream's `flags`): the `LimeLight` maps, the
    /// AFE, the processor's decimation and enable, the low-band filter path and the DC
    /// corrector; then the channels run, the path's band is selected with the board's
    /// switch (an automatic path's following the RX LO), and each channel's bandwidth and
    /// mixer are restored.
    #[must_use]
    pub fn streaming_up_rx<F: Frontend>(
        self,
        frontend: F,
        channels: Mac,
        flags: StreamFlags,
    ) -> Operation<F> {
        self.streaming_up(frontend, (channels, flags), None)
    }

    /// Streams the receive direction up on `rx`'s channels with its stream flags, and
    /// with `tx` the transmit direction too (`lms7002m_streaming_up` with
    /// `RFIC_LMS7_RX`, plus `RFIC_LMS7_TX` for `tx`).
    ///
    /// RX as [`Self::streaming_up_rx`]. Then TX: its processor's interpolation and
    /// enable, the ladder filter path, the front end on with no band, each channel's
    /// bandwidth and mixer restored; then the channels run and the path's band is
    /// selected with the board's switch (an automatic path's following the TX LO). The
    /// `LimeLight` maps and the AFE cover both directions.
    ///
    /// No channel in a direction is [`ControlError::NoChannel`], and a streamed channel
    /// with neither a set bandwidth nor a sample rate [`ControlError::NoRate`], both
    /// before any write or state change. libusdr has already recorded the RX channels and
    /// map when it refuses a TX direction without channels (`lml_rx_chs`, `map_rx`); this
    /// leaves them as they were.
    #[must_use]
    pub fn streaming_up<F: Frontend>(
        mut self,
        frontend: F,
        rx: (Mac, StreamFlags),
        tx: Option<(Mac, StreamFlags)>,
    ) -> Operation<F> {
        let (rx_channels, rx_flags) = rx;
        let planned = self
            .up(Direction::Rx, rx_channels, rx_flags)
            .and_then(|rx_up| {
                let tx_up = tx
                    .map(|(channels, flags)| self.up(Direction::Tx, channels, flags))
                    .transpose()?;
                Ok((rx_up, tx_up))
            });
        let (rx_up, tx_up) = match planned {
            Ok(planned) => planned,
            Err(error) => return Operation::finished(&self, frontend, Err(error)),
        };
        self.rx_map = frontend.lml_map(true, rx_up.channels, rx_flags);
        if let (Some(up), Some((_, flags))) = (tx_up, tx) {
            self.tx_map = frontend.lml_map(false, up.channels, flags);
        }
        let tx_running = tx_up.map_or(self.tx_running, |up| up.running);
        let (siso, maps) = if self.lml.rx_port1 {
            ([self.rx_siso, self.tx_siso], [self.rx_map, self.tx_map])
        } else {
            ([self.tx_siso, self.rx_siso], [self.tx_map, self.rx_map])
        };
        let ([rx_a, rx_b], [tx_a, tx_b]) = (rx_up.running, tx_running);
        let mut calls = Calls::new();
        calls
            .push(ChipCall::LimelightMap(siso, maps))
            .push(ChipCall::AfeEnable(
                [rx_a || rx_b, rx_b],
                [tx_a || tx_b, tx_b],
            ))
            .push(ChipCall::MacSet(rx_up.channels))
            .push(ChipCall::TspIntDec(Tsp::Rx, order(self.clocks.rx_tsp_div)))
            .push(ChipCall::TspEnable(Tsp::Rx, true))
            .push(ChipCall::RbbPath(RbbPath::LowBand, RbbMode::Normal))
            .push(ChipCall::RxtspDcCorr(
                rx_flags.no_dc_correction,
                u3::new(DC_WINDOW),
            ));
        Operation::start(
            self,
            frontend,
            Stage::UpPrepare {
                rx: rx_up,
                tx: tx_up,
            },
            &calls,
        )
    }

    /// One direction of a streaming up (`lms7002m_streaming_up`'s checks and
    /// `_corr_ch`): the banks streamed, the channels that will run and each streamed
    /// channel's bandwidth, its own or, unset, the sample rate's.
    fn up(
        &self,
        direction: Direction,
        channels: Mac,
        flags: StreamFlags,
    ) -> Result<Up, ControlError> {
        if channels == Mac::None {
            return Err(ControlError::NoChannel);
        }
        let clocks = self.clocks;
        let (siso, set, converter_div, tsp_div, board_div) = match direction {
            Direction::Rx => (
                self.rx_siso,
                self.rx_bandwidth,
                clocks.rx_converter_div,
                clocks.rx_tsp_div,
                clocks.rx_decimation,
            ),
            Direction::Tx => (
                self.tx_siso,
                self.tx_bandwidth,
                clocks.tx_converter_div,
                clocks.tx_tsp_div,
                clocks.tx_interpolation,
            ),
        };
        let rate_bandwidth = clocks
            .cgen
            .checked_div(converter_div)
            .and_then(|hz| hz.checked_div(tsp_div))
            .and_then(|hz| hz.checked_div(board_div))
            .and_then(NonZeroU32::new);
        let channels = match channels {
            Mac::Both if siso && flags.swap_ab => Mac::B,
            Mac::Both if siso => Mac::A,
            other => other,
        };
        let running = [channels != Mac::B, channels != Mac::A];
        let mut bandwidths = [None; 2];
        for ((bandwidth, set), streamed) in bandwidths.iter_mut().zip(set).zip(running) {
            if streamed {
                let hertz = set.and_then(NonZeroU32::new).or(rate_bandwidth);
                *bandwidth = Some(hertz.ok_or(ControlError::NoRate)?);
            }
        }
        Ok(Up {
            channels,
            running,
            bandwidths,
        })
    }

    /// Sets which channels run (`rx_run`, `tx_run`), as board code does directly before
    /// streaming up (`limesdr_prepare_streaming`); the run flags decide what later sample
    /// rates and tunings write.
    pub const fn set_running(&mut self, rx: [bool; 2], tx: [bool; 2]) {
        self.rx_running = rx;
        self.tx_running = tx;
    }

    /// Tunes an LO synthesizer to `hertz` (`lms7002m_fe_set_freq`, FDD): 0 stops it,
    /// ignoring write failures as libusdr does. After a lock, a running path's band
    /// follows the new LO: both banks selected, an automatic path's band front end set and
    /// the board's switch moved.
    #[must_use]
    pub fn tune<F: Frontend>(
        self,
        frontend: F,
        synthesizer: Synthesizer,
        hertz: u32,
    ) -> Operation<F> {
        if hertz == 0 {
            let mut calls = Calls::new();
            calls.push(ChipCall::SxxDisable(synthesizer));
            return Operation::start(self, frontend, Stage::TuneOff, &calls);
        }
        let mut operation = Operation::finished(&self, frontend, Ok(()));
        operation.stage = operation.lock(synthesizer, hertz, false);
        operation.settle();
        operation
    }

    /// Tunes both directions to `hertz` on the TX synthesizer (`lms7002m_fe_set_freq`,
    /// `RFIC_LMS7_TX_AND_RX_TDD`): 0 stops the TX synthesizer; otherwise the RX one is
    /// stopped and the TX one locked with its LO shared, write failures before the lock
    /// ignored as libusdr does. After a lock both LOs are `hertz` and each running path
    /// follows, RX first.
    #[must_use]
    pub fn tune_tdd<F: Frontend>(self, frontend: F, hertz: u32) -> Operation<F> {
        let mut calls = Calls::new();
        if hertz == 0 {
            calls.push(ChipCall::SxxDisable(Synthesizer::Tx));
            return Operation::start(self, frontend, Stage::TuneOff, &calls);
        }
        calls.push(ChipCall::SxxDisable(Synthesizer::Rx));
        Operation::start(self, frontend, Stage::TddOff { hertz }, &calls)
    }
}

impl<F: Frontend> Sequence for Operation<F> {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.outcome().is_some() {
            return None;
        }
        self.part.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        match self.part.complete(&mut self.control.chip, result) {
            Ok(()) => {}
            // `lms7002m_fe_set_freq` ignores `lms7002m_sxx_disable`'s result.
            Err(_) if matches!(self.stage, Stage::TuneOff) => {
                self.stage = Stage::Done(Ok(()));
                return Ok(());
            }
            Err(_) if matches!(self.stage, Stage::TddOff { .. }) => self.advance(),
            Err(failure) => {
                self.failed = true;
                return Err(failure);
            }
        }
        self.settle();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{FpgaAccess, SpiAddr};

    use super::*;
    use crate::chip::LmlMap;
    use crate::chip::Lms7002m;
    use crate::ctrl::{Clocks, RateFlags, RxAntenna, TxAntenna};
    use crate::reg::Reg;
    extern crate std;
    use crate::regs::{LmlControl, LmlSample};
    use std::vec::Vec;

    /// A created chip's control state on a 26 MHz reference.
    fn control() -> Control {
        let rx = RxAntenna {
            below_hz: u64::MAX,
            path: RxPath::LnaW,
            switch: 0,
            loopback_switch: 1,
        };
        let tx = TxAntenna {
            below_hz: u64::MAX,
            path: TxPath::Band1,
            switch: 0,
            loopback_switch: 1,
            names: ["W", "B1"],
        };
        Control::new(
            Lms7002m::created(SpiAddr(0)),
            NonZeroU32::new(26_000_000).unwrap(),
            [rx; 3],
            [tx; 2],
        )
    }

    /// A one-rate request.
    fn request(rx: u32, rx_decimation: u32) -> RateRequest {
        RateRequest {
            rx,
            tx: 0,
            adc: 0,
            dac: 0,
            flags: RateFlags::default(),
            rx_port1: true,
            rx_decimation,
            tx_interpolation: 1,
        }
    }

    #[test]
    fn rate_requests_libusdr_would_hang_or_divide_by_zero_on_are_refused_before_any_write() {
        for (rx, decimation) in [(0, 1), (1_000_000, 0), (1_000_000, 2_000_000)] {
            let op = control().sample_rate(request(rx, decimation));
            assert_eq!(
                op.next(),
                None,
                "RX {rx}, decimation {decimation}: no write"
            );
            assert_eq!(op.outcome(), Some(Err(ControlError::InvalidRate)));
        }
    }

    #[test]
    fn streaming_up_without_a_rate_or_bandwidth_is_refused_before_any_write() {
        let op = control().streaming_up_rx(NoFrontend(()), Mac::Both, StreamFlags::default());
        assert_eq!(op.next(), None);
        assert_eq!(op.outcome(), Some(Err(ControlError::NoRate)));
    }

    #[test]
    fn a_transmit_direction_without_a_channel_or_rate_refuses_the_whole_streaming_up() {
        let flags = StreamFlags::default();
        let mut rated = control();
        rated.clocks = Clocks {
            cgen: 32_000_000,
            rx_converter_div: 4,
            rx_tsp_div: 8,
            rx_decimation: 1,
            ..Clocks::default()
        };
        for (tx, refusal) in [
            (Mac::None, ControlError::NoChannel),
            (Mac::A, ControlError::NoRate),
        ] {
            let op = rated.streaming_up(NoFrontend(()), (Mac::A, flags), Some((tx, flags)));
            assert_eq!(op.next(), None, "TX on {tx:?}: no write, RX's included");
            assert_eq!(op.outcome(), Some(Err(refusal)));
            assert_eq!(op.control().rx_running(), [false; 2], "nothing runs");
        }
    }

    /// A board whose antenna switch is one gateware write: register 0 for RX, 1 for TX,
    /// of the position.
    #[derive(Clone, Copy, Debug)]
    struct Switches;

    /// [`Switches`]' switch.
    #[derive(Clone, Copy, Debug)]
    struct Switch(Option<FpgaAccess>);

    impl Sequence for Switch {
        fn next(&self) -> Option<SequenceAction> {
            self.0.map(SequenceAction::Fpga)
        }

        fn complete<E>(&mut self, _result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
            self.0 = None;
            Ok(())
        }
    }

    impl Frontend for Switches {
        type Switch = Switch;

        fn antenna_switch(&self, direction: Direction, position: u8) -> Switch {
            Switch(Some(FpgaAccess::Write {
                reg: u32::from(direction == Direction::Tx),
                value: position.into(),
            }))
        }

        fn lml_map(&self, _rx: bool, _channels: Mac, _flags: StreamFlags) -> LmlMap {
            [LmlSample::AI, LmlSample::AQ, LmlSample::BI, LmlSample::BQ]
        }
    }

    /// What an operation did: each LMS7002M write (address, value) and each switch
    /// (`u16::MAX - register`, position), in order.
    fn run<F: Frontend>(mut op: Operation<F>) -> (Vec<(u16, u16)>, Operation<F>) {
        let mut done = Vec::new();
        while let Some(action) = op.next() {
            match action {
                SequenceAction::Fpga(FpgaAccess::Spi { word, .. }) if word >> 31 == 1 => {
                    let [high, low] = [word >> 16 & 0x7fff, word & 0xffff]
                        .map(|half| u16::try_from(half).expect("16 bits"));
                    done.push((high, low));
                }
                SequenceAction::Fpga(FpgaAccess::Write { reg, value }) => done.push((
                    u16::MAX - u16::try_from(reg).expect("0 or 1"),
                    u16::try_from(value).expect("a position"),
                )),
                _ => {}
            }
            op.complete::<()>(Ok(0)).expect("no failure");
        }
        (done, op)
    }

    /// The bank selection the last `LML_0x0020` write before `at` made.
    fn mac_before(done: &[(u16, u16)], at: usize) -> Mac {
        let &(_, value) = done[..at]
            .iter()
            .rev()
            .find(|(addr, _)| *addr == Reg::LmlControl as u16)
            .expect("a bank selection");
        LmlControl::from_raw(value).mac()
    }

    /// A created chip with RX and TX at 1 MS/s, its TX streamed SISO when `tx_siso`.
    fn rated(tx_siso: bool) -> Control {
        let mut rated = control();
        rated.clocks = Clocks {
            cgen: 32_000_000,
            rx_converter_div: 4,
            tx_converter_div: 4,
            rx_tsp_div: 8,
            tx_tsp_div: 8,
            rx_decimation: 1,
            tx_interpolation: 1,
        };
        rated.tx_siso = tx_siso;
        rated
    }

    #[test]
    fn the_transmit_half_selects_its_own_banks_before_its_processor_and_its_band() {
        let swapped = StreamFlags {
            swap_ab: true,
            ..StreamFlags::default()
        };
        for (tx_siso, banks, running) in [
            (false, Mac::Both, [true, true]),
            (true, Mac::B, [false, true]),
        ] {
            let op = rated(tx_siso).streaming_up(
                Switches,
                (Mac::A, StreamFlags::default()),
                Some((Mac::Both, swapped)),
            );
            let (done, op) = run(op);
            assert_eq!(op.outcome(), Some(Ok(())), "SISO {tx_siso}");
            let tx_processor = done
                .iter()
                .position(|(addr, _)| *addr == Reg::TxTspBypass as u16)
                .expect("TX processor writes");
            assert_eq!(
                mac_before(&done, tx_processor),
                banks,
                "SISO {tx_siso}: TX's banks (A after RX), not RX's"
            );
            assert!(
                done[..tx_processor]
                    .iter()
                    .any(|&(addr, _)| addr == u16::MAX),
                "RX's switch first"
            );
            let band = done
                .iter()
                .rposition(|(addr, _)| *addr == Reg::TxBand as u16)
                .expect("the band write");
            assert_eq!(
                mac_before(&done, band),
                banks,
                "SISO {tx_siso}: the band on TX's banks again, after the restore loop"
            );
            assert_eq!(
                done.last(),
                Some(&(u16::MAX - 1, 0)),
                "TX's switch last, the only band's"
            );
            assert_eq!(op.control().tx_running(), running, "SISO {tx_siso}");
            assert_eq!(op.control().rx_running(), [true, false]);
        }
    }
}
