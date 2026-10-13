//! The control code's operations as one hand-written machine: each runs its parts (chip
//! calls, tunings, board switches) in libusdr's order, holding the [`Control`] state.

use bnb::u3;
use core::num::NonZeroU32;

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use super::calls::{Calls, ChipCall};
use super::part::Part;
use super::rate::{Candidate, RateRequest};
use super::{
    Control, Direction, Frontend, NoFrontend, StreamFlags, band_for, order, push_bandwidth,
};
use crate::chip::{LmlConfig, RbbMode, RbbPath, RxMode, Synthesizer, Tsp, TuneError, TxMode};
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

    /// Streaming up RX: the map, AFE, processor and baseband writes on `channels`.
    UpPrepare {
        /// The channels that will run.
        running: [bool; 2],
        /// The banks streamed.
        channels: Mac,
        /// Each streamed channel's bandwidth.
        bandwidths: [Option<NonZeroU32>; 2],
    },

    /// Streaming up RX: the band's LNA selected; its switch follows.
    UpBand {
        /// The banks streamed.
        channels: Mac,
        /// Each streamed channel's bandwidth.
        bandwidths: [Option<NonZeroU32>; 2],
        /// The switch position.
        switch: u8,
    },

    /// Streaming up RX: the antenna switch.
    UpSwitch {
        /// The banks streamed.
        channels: Mac,
        /// Each streamed channel's bandwidth, checked non-zero before any write.
        bandwidths: [Option<NonZeroU32>; 2],
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
            Stage::UpPrepare {
                running,
                channels,
                bandwidths,
            } => {
                self.control.rx_running = running;
                let band = band_for(
                    self.control.rx_lo,
                    self.control.rx_bands.iter().map(|band| band.below_hz),
                );
                let antenna = self.control.rx_bands[band];
                let mut calls = Calls::new();
                calls.push(ChipCall::RfePath(antenna.path, RxMode::Normal));
                self.part = Part::calls(calls, &mut self.control.chip);
                Stage::UpBand {
                    channels,
                    bandwidths,
                    switch: antenna.switch,
                }
            }
            Stage::UpBand {
                channels,
                bandwidths,
                switch,
            } => {
                self.part = Part::Board(self.frontend.antenna_switch(Direction::Rx, switch));
                Stage::UpSwitch {
                    channels,
                    bandwidths,
                }
            }
            Stage::UpSwitch {
                channels,
                bandwidths,
            } => {
                let calls = restore_rx(channels, bandwidths);
                self.part = Part::calls(calls, &mut self.control.chip);
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

/// `lms7002m_streaming_up`'s restore loop for RX: per streamed channel, its bank, its
/// bandwidth (set, or the sample rate's) and the mixer bypassed (no NCO offset is ported).
fn restore_rx(channels: Mac, bandwidths: [Option<NonZeroU32>; 2]) -> Calls {
    let mut calls = Calls::new();
    for (bank, bandwidth) in [Mac::A, Mac::B].into_iter().zip(bandwidths) {
        let Some(bandwidth) = bandwidth else {
            continue;
        };
        if matches!(
            (channels, bank),
            (Mac::Both, _) | (Mac::A, Mac::A) | (Mac::B, Mac::B)
        ) {
            calls.push(ChipCall::MacSet(bank));
            push_bandwidth(&mut calls, Direction::Rx, bandwidth);
            calls.push(ChipCall::TspCmix(Tsp::Rx, 0));
        }
    }
    calls
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
        mut self,
        frontend: F,
        channels: Mac,
        flags: StreamFlags,
    ) -> Operation<F> {
        if channels == Mac::None {
            return Operation::finished(self, frontend, Err(ControlError::NoChannel));
        }
        let clocks = self.clocks;
        let rate_bandwidth = clocks
            .cgen
            .checked_div(clocks.rx_converter_div)
            .and_then(|hz| hz.checked_div(clocks.rx_tsp_div))
            .and_then(|hz| hz.checked_div(clocks.rx_decimation))
            .and_then(NonZeroU32::new);
        let channels = match channels {
            Mac::Both if self.rx_siso && flags.swap_ab => Mac::B,
            Mac::Both if self.rx_siso => Mac::A,
            other => other,
        };
        let running = [channels != Mac::B, channels != Mac::A];
        let mut bandwidths = [None; 2];
        for ((bandwidth, set), streamed) in
            bandwidths.iter_mut().zip(self.rx_bandwidth).zip(running)
        {
            if !streamed {
                continue;
            }
            let Some(hertz) = set.and_then(NonZeroU32::new).or(rate_bandwidth) else {
                return Operation::finished(self, frontend, Err(ControlError::NoRate));
            };
            *bandwidth = Some(hertz);
        }
        self.rx_map = frontend.lml_map(true, channels, flags);
        let [tx_a, tx_b] = self.tx_running;
        let (siso, maps) = if self.lml.rx_port1 {
            ([self.rx_siso, self.tx_siso], [self.rx_map, self.tx_map])
        } else {
            ([self.tx_siso, self.rx_siso], [self.tx_map, self.rx_map])
        };
        let mut calls = Calls::new();
        calls
            .push(ChipCall::LimelightMap(siso, maps))
            .push(ChipCall::AfeEnable(
                [running[0] || running[1], running[1]],
                [tx_a || tx_b, tx_b],
            ))
            .push(ChipCall::MacSet(channels))
            .push(ChipCall::TspIntDec(Tsp::Rx, order(clocks.rx_tsp_div)))
            .push(ChipCall::TspEnable(Tsp::Rx, true))
            .push(ChipCall::RbbPath(RbbPath::LowBand, RbbMode::Normal))
            .push(ChipCall::RxtspDcCorr(
                flags.no_dc_correction,
                u3::new(DC_WINDOW),
            ));
        Operation::start(
            self,
            frontend,
            Stage::UpPrepare {
                running,
                channels,
                bandwidths,
            },
            &calls,
        )
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
    use rsl_lowlevel::SpiAddr;

    use super::*;
    use crate::TxPath;
    use crate::chip::Lms7002m;
    use crate::ctrl::{RateFlags, RxAntenna, TxAntenna};
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
}
