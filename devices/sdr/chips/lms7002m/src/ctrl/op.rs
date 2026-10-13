//! The control code's operations as one hand-written machine: each runs its parts (chip
//! calls, tunings, board switches) in libusdr's order, holding the [`Control`] state.

use bnb::u3;
use core::num::NonZeroU32;

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::calls::{Calls, ChipCall};
use super::part::Part;
use super::rate::{Candidate, RateRequest};
use super::{
    Control, Direction, Frontend, NoFrontend, StreamFlags, band_for, order, push_bandwidth,
};
use crate::chip::{
    LmlConfig, RbbMode, RbbPath, RxMode, Synthesizer, TbbMode, TbbPath, Tsp, TuneError, TxMode,
    TxPath,
};
use crate::regs::Mac;

/// The RX DC corrector's window `lms7002m_streaming_up` sets.
const DC_WINDOW: u8 = 7;
/// CGEN tunings per candidate before libusdr tries the next.
const CGEN_ATTEMPTS: u8 = 4;

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

    /// Streaming up before any sample rate, where libusdr divides by zero; refused before
    /// any write.
    NoRate,

    /// A rate request libusdr would hang or divide by zero on: no rate in either
    /// direction with no converter floor (CGEN 0), a zero board decimation or
    /// interpolation, or one larger than its rate. Refused before any write.
    InvalidRate,
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

    /// Streaming up RX: the band's LNA selected; its switch follows.
    UpBand {
        /// The receive direction.
        rx: Up,
        /// The transmit direction, when it streams up too.
        tx: Option<Up>,
        /// The switch position.
        switch: u8,
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

    /// Streaming up TX: the banks reselected and the band's output set; its switch
    /// follows.
    UpTxBand {
        /// The switch position.
        switch: u8,
    },

    /// Stopping a synthesizer, whose write failures libusdr ignores.
    TuneOff,

    /// Locking a synthesizer.
    TuneLock {
        /// The synthesizer.
        synthesizer: Synthesizer,
        /// The LO asked.
        hertz: u32,
    },

    /// After a lock with the path running: both banks selected and the band's front end
    /// set; its switch follows.
    TuneBand {
        /// The direction.
        direction: Direction,
        /// The switch position.
        switch: u8,
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

    /// A bus access failed.
    failed: bool,
}

impl Operation<NoFrontend> {
    /// Runs `calls` and finishes.
    pub(super) fn writes(control: Control, calls: &Calls) -> Self {
        Self::start(control, NoFrontend(()), Stage::Last, calls)
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
            failed: false,
        };
        operation.settle();
        operation
    }

    /// An operation that finished before any access.
    fn finished(control: Control, frontend: F, outcome: Result<(), ControlError>) -> Self {
        Self::start(control, frontend, Stage::Done(outcome), &Calls::new())
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
                let band = band_for(
                    self.control.rx_lo,
                    self.control.rx_bands.iter().map(|band| band.below_hz),
                );
                let antenna = self.control.rx_bands[band];
                let mut calls = Calls::new();
                calls.push(ChipCall::RfePath(antenna.path, RxMode::Normal));
                self.part = Part::calls(calls, &mut self.control.chip);
                Stage::UpBand {
                    rx,
                    tx,
                    switch: antenna.switch,
                }
            }
            Stage::UpBand { rx, tx, switch } => {
                self.part = Part::Board(self.frontend.antenna_switch(Direction::Rx, switch));
                Stage::UpSwitch { rx, tx }
            }
            Stage::UpSwitch { rx, tx } => {
                let mut calls = Calls::new();
                push_restore(&mut calls, Direction::Rx, rx);
                self.part = Part::calls(calls, &mut self.control.chip);
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
                push_restore(&mut calls, Direction::Tx, tx);
                self.part = Part::calls(calls, &mut self.control.chip);
                Stage::UpTxPrepare { tx }
            }
            Stage::UpTxPrepare { tx } => {
                // libusdr sets the run flags before reselecting the banks.
                self.control.tx_running = tx.running;
                let band = band_for(
                    self.control.tx_lo,
                    self.control.tx_bands.iter().map(|band| band.below_hz),
                );
                let antenna = self.control.tx_bands[band];
                let mut calls = Calls::new();
                calls
                    .push(ChipCall::MacSet(tx.channels))
                    .push(ChipCall::TrfPath(antenna.path, TxMode::Normal));
                self.part = Part::calls(calls, &mut self.control.chip);
                Stage::UpTxBand {
                    switch: antenna.switch,
                }
            }
            Stage::UpTxBand { switch } => {
                self.part = Part::Board(self.frontend.antenna_switch(Direction::Tx, switch));
                Stage::Last
            }
            Stage::TuneLock { synthesizer, hertz } => self.after_lock(synthesizer, hertz),
            Stage::TuneBand { direction, switch } => {
                self.part = Part::Board(self.frontend.antenna_switch(direction, switch));
                Stage::Last
            }
            Stage::Done(outcome) => Stage::Done(outcome),
        };
        self.stage = next;
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
            self.part = Part::calls(calls, &mut self.control.chip);
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

    /// The stage after an LO synthesizer's tuning finished: on a lock with the path
    /// running, the band follows the new LO (libusdr's `XSDR_*_LO_CHANGED` signal).
    fn after_lock(&mut self, synthesizer: Synthesizer, hertz: u32) -> Stage {
        let Part::Sxx(tune) = self.part else {
            return Stage::Done(Err(ControlError::Tune(TuneError::OutOfRange)));
        };
        match tune.outcome() {
            Some(Ok(())) => {}
            Some(Err(error)) => return Stage::Done(Err(ControlError::Tune(error))),
            None => return Stage::Done(Err(ControlError::Tune(TuneError::OutOfRange))),
        }
        let control = &mut self.control;
        let mut calls = Calls::new();
        calls.push(ChipCall::MacSet(Mac::Both));
        let (direction, switch) = match synthesizer {
            Synthesizer::Rx => {
                control.rx_lo = hertz;
                if !control.rx_running.contains(&true) {
                    return Stage::Done(Ok(()));
                }
                let band = band_for(hertz, control.rx_bands.iter().map(|band| band.below_hz));
                let antenna = control.rx_bands[band];
                calls.push(ChipCall::RfePath(antenna.path, RxMode::Normal));
                (Direction::Rx, antenna.switch)
            }
            Synthesizer::Tx => {
                control.tx_lo = hertz;
                if !control.tx_running.contains(&true) {
                    return Stage::Done(Ok(()));
                }
                let band = band_for(hertz, control.tx_bands.iter().map(|band| band.below_hz));
                let antenna = control.tx_bands[band];
                calls.push(ChipCall::TrfPath(antenna.path, TxMode::Normal));
                (Direction::Tx, antenna.switch)
            }
        };
        self.part = Part::calls(calls, &mut self.control.chip);
        Stage::TuneBand { direction, switch }
    }
}

/// `lms7002m_streaming_up`'s restore loop for one direction: per streamed channel, its
/// bank, its bandwidth (set, or the sample rate's) and the mixer bypassed (no NCO offset is
/// ported).
fn push_restore(calls: &mut Calls, direction: Direction, up: Up) {
    let tsp = match direction {
        Direction::Rx => Tsp::Rx,
        Direction::Tx => Tsp::Tx,
    };
    for (bank, bandwidth) in [Mac::A, Mac::B].into_iter().zip(up.bandwidths) {
        // Only streamed channels have one.
        let Some(bandwidth) = bandwidth else {
            continue;
        };
        calls.push(ChipCall::MacSet(bank));
        push_bandwidth(calls, direction, bandwidth);
        calls.push(ChipCall::TspCmix(tsp, 0));
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
            return Operation::finished(self, frontend, Err(ControlError::InvalidRate));
        }
        let candidate = match request.candidate(0) {
            Ok(candidate) => candidate,
            Err(error) => return Operation::finished(self, frontend, Err(error)),
        };
        let mut operation = Operation::finished(self, frontend, Ok(()));
        operation.stage = operation.tune_cgen(request, 0, 0, candidate);
        operation.settle();
        operation
    }

    /// Streams `channels` up in the receive direction (the RX half of
    /// `lms7002m_streaming_up`, with the stream's `flags`): the `LimeLight` maps, the
    /// AFE, the processor's decimation and enable, the low-band filter path and the DC
    /// corrector; then the channels run, the band follows the RX LO with the board's
    /// switch, and each channel's bandwidth and mixer are restored.
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
    /// bandwidth and mixer restored; then the channels run and the band follows the TX LO
    /// with the board's switch. The `LimeLight` maps and the AFE cover both directions.
    ///
    /// No channel in a direction is [`ControlError::NoChannel`], and a streamed channel
    /// with neither a set bandwidth nor a sample rate [`ControlError::NoRate`], both
    /// before any write or state change.
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
            Err(error) => return Operation::finished(self, frontend, Err(error)),
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
    /// follows the new LO: both banks selected, the band's front end set and the board's
    /// switch moved.
    #[must_use]
    pub fn tune<F: Frontend>(
        mut self,
        frontend: F,
        synthesizer: Synthesizer,
        hertz: u32,
    ) -> Operation<F> {
        if hertz == 0 {
            let mut calls = Calls::new();
            calls.push(ChipCall::SxxDisable(synthesizer));
            return Operation::start(self, frontend, Stage::TuneOff, &calls);
        }
        let tune = self
            .chip
            .sxx_tune(synthesizer, self.reference, hertz, false);
        let mut operation = Operation::finished(self, frontend, Ok(()));
        operation.part = Part::Sxx(tune);
        operation.stage = Stage::TuneLock { synthesizer, hertz };
        operation.settle();
        operation
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
    use hal::SpiAddr;

    use super::*;
    use crate::TxPath;
    use crate::chip::Lms7002m;
    use crate::ctrl::{Clocks, RateFlags, RxAntenna, TxAntenna};
    use crate::regs::RxPath;

    /// A created chip's control state on a 26 MHz reference.
    fn control() -> Control {
        let rx = RxAntenna {
            below_hz: u64::MAX,
            path: RxPath::LnaW,
            switch: 0,
        };
        let tx = TxAntenna {
            below_hz: u64::MAX,
            path: TxPath::Band1,
            switch: 0,
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
}
