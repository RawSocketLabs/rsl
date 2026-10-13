//! The DC-offset calibration engines (`lms6002d_calibration_loop`) and libusdr's RX DC
//! calibration at stream creation, which runs them (`_usdr_lms6002_dc_calib_rx`): the
//! LPF's DC DAC, the LPF tuning for every bandwidth code (tuning the TX PLL to 320 MHz
//! first), the TX and RX LPF offsets, RXVGA2's offsets, then RXVGA2's gain back.

use core::time::Duration;

use bnb::{u3, u4, u6};

use super::super::script::Script;
use super::super::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use super::afe::ConverterEnables;
use super::chip::{Lms6002d, NAME};
use super::dc_cal::{DcBlock, DcControl, DcStatus, DcValue};
use super::lpf::{self, Lpf};
use super::pll::Pll;
use super::rx_fe::{MixerBias, MixerInput};
use super::rx_vga2;
use super::spi;
use super::top::{LpfTuning, LpfTuningControl};
use super::tune::{PllTune, TuneFault};

/// The polls before libusdr gives up on an engine.
const POLLS: u8 = 100;
/// A saturated engine result.
const SATURATED: u8 = 0x1f;
/// The TX LO libusdr tunes to before the LPF tuning, in Hz.
const LPF_TUNING_TX_LO_HZ: u32 = 320_000_000;

/// How a calibration loop ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoopEnd {
    /// The engine finished with this unsaturated value.
    Converged(u8),
    /// It did not within its polls.
    Unconverged,
}

/// Where a calibration loop is.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed-size stages, no allocation: one per loop"
)]
enum LoopStage {
    /// Starting the engine from a value.
    Start(Script),
    /// Waiting 7 µs before a poll.
    Wait,
    /// Reading the status.
    Status,
    /// Reading the value of a finished run.
    Value,
    /// Finished, or `None` after a bus failure.
    Done(Option<LoopEnd>),
}

/// libusdr's `lms6002d_calibration_loop`: starts `block`'s engine on `channel` from 31,
/// then polls every 7 µs, up to 100 times, until it finishes with an unsaturated value. A
/// saturated result (0x1F) restarts the engine from 0.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DcLoop {
    /// The chip, for its SPI target.
    lms: Lms6002d,
    /// The engine.
    block: DcBlock,
    /// Its channel.
    channel: u8,
    /// Polls begun.
    polls: u8,
    /// Where it is.
    stage: LoopStage,
}

impl DcLoop {
    /// The loop on `block`'s `channel`.
    pub(super) fn new(lms: Lms6002d, block: DcBlock, channel: u8) -> Self {
        let mut run = Self {
            lms,
            block,
            channel,
            polls: 0,
            stage: LoopStage::Done(None),
        };
        run.stage = LoopStage::Start(run.start(SATURATED));
        run
    }

    /// The engine's start from `from`: the value, the channel, a start pulse.
    fn start(&self, from: u8) -> Script {
        let control = DcControl::new()
            .with_released(true)
            .with_channel(u3::new(self.channel));
        let lms = self.lms;
        Script::new()
            .fpga(
                NAME,
                lms.write_at(self.block, DcValue::new().with_value(u6::new(from))),
            )
            .fpga(NAME, lms.write_at(self.block, control))
            .fpga(NAME, lms.write_at(self.block, control.with_start(true)))
            .fpga(NAME, lms.write_at(self.block, control))
    }

    /// How the loop ended; `None` while running or after a bus failure.
    pub(super) const fn converged(&self) -> Option<LoopEnd> {
        match self.stage {
            LoopStage::Done(converged) => converged,
            _ => None,
        }
    }

    /// The next poll, or the end once all were taken.
    fn poll(&mut self) -> LoopStage {
        if self.polls == POLLS {
            return LoopStage::Done(Some(LoopEnd::Unconverged));
        }
        self.polls += 1;
        LoopStage::Wait
    }
}

impl Sequence for DcLoop {
    fn next(&self) -> Option<SequenceAction> {
        let access = match self.stage {
            LoopStage::Start(script) => return script.next(),
            LoopStage::Wait => return Some(SequenceAction::Sleep(Duration::from_micros(7))),
            LoopStage::Status => self.lms.read_at::<DcStatus>(self.block),
            LoopStage::Value => self.lms.read_at::<DcValue>(self.block),
            LoopStage::Done(_) => return None,
        };
        Some(SequenceAction::Fpga(access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = match &mut self.stage {
            LoopStage::Start(script) => {
                let started = script.complete(result);
                let finished = script.next().is_none();
                if started.is_err() {
                    self.stage = LoopStage::Done(None);
                } else if finished {
                    self.stage = self.poll();
                }
                return started;
            }
            _ => match super::super::script::checked(NAME, action, result) {
                Ok(word) => spi::value(word),
                Err(failure) => {
                    self.stage = LoopStage::Done(None);
                    return Err(failure);
                }
            },
        };
        self.stage = match self.stage {
            LoopStage::Wait => LoopStage::Status,
            LoopStage::Status if DcStatus::from(word).running() => self.poll(),
            LoopStage::Status => LoopStage::Value,
            LoopStage::Value => match DcValue::from(word).to_raw() & SATURATED {
                SATURATED => LoopStage::Start(self.start(0)),
                value => LoopStage::Done(Some(LoopEnd::Converged(value))),
            },
            stage @ (LoopStage::Start(_) | LoopStage::Done(_)) => stage,
        };
        Ok(())
    }
}

/// What a calibration step runs.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// Register writes or a read.
    Script(Script),
    /// An engine's loop.
    Loop(DcLoop),
    /// The TX PLL's tuning.
    Tune(PllTune),
}

impl Part {
    /// The part's next action.
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Script(script) => script.next(),
            Self::Loop(run) => run.next(),
            Self::Tune(tune) => tune.next(),
        }
    }

    /// Applies an action's outcome.
    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        match self {
            Self::Script(script) => script.complete(result),
            Self::Loop(run) => run.complete(result),
            Self::Tune(tune) => tune.complete(result),
        }
    }
}

/// One bandwidth code's LPF tuning (`lms6002d_cal_lpf_bandwidth`), step by step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LpfStep {
    /// The transmitter's enables on.
    Enables,
    /// The TX PLL to 320 MHz (code 0 only).
    Tune,
    /// The tuning engine's control writes.
    Engine,
    /// The 10 µs wait, taken even after a failure.
    Wait,
    /// The engine's status read; libusdr keeps the RC value only for its log.
    Status,
    /// The clock off and the enables back.
    Close,
}

/// Where the RX DC calibration is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// The DC DAC calibration's setup.
    DacSetup,
    /// Its engine loop, whose failures libusdr discards.
    DacLoop,
    /// Its result written to both filters.
    DacBypass,
    /// Its cleanup.
    DacCleanup,
    /// The LPF tuning for a bandwidth code.
    Lpf {
        /// The code.
        code: u8,
        /// The step.
        step: LpfStep,
        /// The transmitter was on before.
        tx_was_on: bool,
    },
    /// A filter's DC offsets: the setup's clock enables.
    FilterSetup(Lpf),
    /// The I channel's loop.
    FilterI(Lpf),
    /// The Q channel's loop, once I converged.
    FilterQ(Lpf),
    /// The cleanup's clock enables.
    FilterCleanup(Lpf),
    /// The mixer's external input terminated.
    TerminationOn,
    /// RXVGA2's calibration setup.
    Vga2Setup,
    /// RXVGA2's channel loops: 0 the reference, 1 and 2 stage A, 3 and 4 stage B.
    Vga2Loop(u8),
    /// Stage B at 10 before its channels.
    Vga2StageB,
    /// RXVGA2's cleanup.
    Vga2Cleanup,
    /// The termination undone.
    TerminationOff,
    /// RXVGA2's gain back.
    Gain,
    /// Finished, or stopped at a failure or fault.
    Done,
}

/// libusdr's RX DC-offset calibration at stream creation (`_usdr_lms6002_dc_calib_rx`).
/// It owns the chip's cache, which it updates; take it back with [`Self::chip`].
///
/// Like libusdr, a calibration that does not converge is not an error, and the TX
/// synthesizer's modulator clock stays on afterwards. Bus failures inside the engine loops
/// are discarded, as libusdr discards them; elsewhere the first ends the calibration. The
/// TX PLL's tuning faults end it with [`Self::fault`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct RxDcCalibration {
    /// The chip.
    lms: Lms6002d,
    /// The reference, in Hz, for the TX PLL's tuning.
    reference_hz: u32,
    /// RXVGA2's stage A and B steps to restore.
    vga2: (u8, u8),
    /// Where it is.
    stage: Stage,
    /// The part running.
    part: Part,
    /// The current LPF tuning failed; its wait still runs.
    lpf_failed: bool,
    /// A tuning fault ended it.
    fault: Option<TuneFault>,
}

impl RxDcCalibration {
    /// The calibration, from a `reference_hz` reference, restoring RXVGA2 to `vga2` (stage
    /// A, stage B).
    pub(crate) fn new(lms: Lms6002d, reference_hz: u32, vga2: (u8, u8)) -> Self {
        let mut calibration = Self {
            lms,
            reference_hz,
            vga2,
            stage: Stage::DacSetup,
            part: Part::Script(Script::new()),
            lpf_failed: false,
            fault: None,
        };
        calibration
            .lms
            .clock_enables
            .set_lpf_calibration_clock(true);
        calibration.part = Self::script(&[
            calibration.lms.write(ConverterEnables::CALIBRATING),
            calibration.lms.write(calibration.lms.clock_enables),
        ]);
        calibration
    }

    /// The chip's cache as the calibration has left it so far.
    pub(crate) const fn chip(&self) -> &Lms6002d {
        &self.lms
    }

    /// The TX PLL tuning's fault that ended the calibration, if one did.
    pub(crate) const fn fault(&self) -> Option<TuneFault> {
        self.fault
    }

    /// A script of `accesses`.
    fn script(accesses: &[FpgaAccess]) -> Part {
        Part::Script(
            accesses
                .iter()
                .fold(Script::new(), |script, &access| script.fpga(NAME, access)),
        )
    }

    /// Whether this stage's part discards its failures.
    const fn lenient(&self) -> bool {
        matches!(
            self.stage,
            Stage::DacLoop
                | Stage::FilterI(_)
                | Stage::FilterQ(_)
                | Stage::Vga2Setup
                | Stage::Vga2Loop(_)
                | Stage::Vga2StageB
        )
    }

    /// Moves past finished parts until one has an action or the calibration is done.
    fn settle(&mut self) {
        while self.stage != Stage::Done && self.part.next().is_none() {
            self.advance(true);
        }
    }

    /// The stage after the current part ended, `ok` if without a discarded failure.
    fn advance(&mut self, ok: bool) {
        let lms = self.lms;
        let (stage, part) = match self.stage {
            Stage::DacSetup => (
                Stage::DacLoop,
                Part::Loop(DcLoop::new(lms, DcBlock::Lpf, 0)),
            ),
            Stage::DacLoop => match self.part {
                Part::Loop(run) if ok => match run.converged() {
                    Some(LoopEnd::Converged(value)) => {
                        let bypass = lpf::Bypass::new().with_dc_dac_calibration(u6::new(value));
                        (
                            Stage::DacBypass,
                            Self::script(&[
                                lms.write_at(Lpf::Rx, bypass),
                                lms.write_at(Lpf::Tx, bypass),
                            ]),
                        )
                    }
                    _ => self.dac_cleanup(),
                },
                _ => self.dac_cleanup(),
            },
            Stage::DacBypass => self.dac_cleanup(),
            Stage::DacCleanup => self.lpf(0),
            Stage::Lpf {
                code,
                step,
                tx_was_on,
            } => self.lpf_step(code, step, tx_was_on),
            Stage::FilterSetup(lpf) => (
                Stage::FilterI(lpf),
                Part::Loop(DcLoop::new(lms, filter_block(lpf), 0)),
            ),
            Stage::FilterI(lpf) => match self.part {
                Part::Loop(run) if ok && matches!(run.converged(), Some(LoopEnd::Converged(_))) => {
                    (
                        Stage::FilterQ(lpf),
                        Part::Loop(DcLoop::new(lms, filter_block(lpf), 1)),
                    )
                }
                _ => self.filter_cleanup(lpf),
            },
            Stage::FilterQ(lpf) => self.filter_cleanup(lpf),
            Stage::FilterCleanup(Lpf::Tx) => (Stage::TerminationOn, self.termination(true)),
            Stage::TerminationOn => self.filter_setup(Lpf::Rx),
            Stage::FilterCleanup(Lpf::Rx) => {
                self.lms
                    .clock_enables
                    .set_rx_vga2_dc_calibration_clock(true);
                let lms = self.lms;
                (
                    Stage::Vga2Setup,
                    Self::script(&[
                        lms.write(rx_vga2::CalibrationPower::new()),
                        lms.write(lms.clock_enables),
                        lms.write(stage_a()),
                    ]),
                )
            }
            Stage::Vga2Setup if ok => (
                Stage::Vga2Loop(0),
                Part::Loop(DcLoop::new(lms, DcBlock::RxVga2, 0)),
            ),
            Stage::Vga2Loop(channel) => match self.part {
                Part::Loop(run) if ok && matches!(run.converged(), Some(LoopEnd::Converged(_))) => {
                    match channel {
                        2 => (
                            Stage::Vga2StageB,
                            Self::script(&[lms.write(rx_vga2::Gain::new().with_b(u4::new(10)))]),
                        ),
                        4 => self.vga2_cleanup(),
                        _ => (
                            Stage::Vga2Loop(channel + 1),
                            Part::Loop(DcLoop::new(lms, DcBlock::RxVga2, channel + 1)),
                        ),
                    }
                }
                _ => self.vga2_cleanup(),
            },
            Stage::Vga2StageB if ok => (
                Stage::Vga2Loop(3),
                Part::Loop(DcLoop::new(lms, DcBlock::RxVga2, 3)),
            ),
            Stage::Vga2Setup | Stage::Vga2StageB => self.vga2_cleanup(),
            Stage::Vga2Cleanup => (Stage::TerminationOff, self.termination(false)),
            Stage::TerminationOff => {
                let (a, b) = self.vga2;
                (Stage::Gain, Part::Script(lms.set_rx_vga2_gain(a, b)))
            }
            Stage::Gain | Stage::Done => (Stage::Done, Part::Script(Script::new())),
        };
        self.stage = stage;
        self.part = part;
    }

    /// The DC DAC calibration's cleanup: its clock off, the comparators off.
    fn dac_cleanup(&mut self) -> (Stage, Part) {
        self.lms.clock_enables.set_lpf_calibration_clock(false);
        let lms = self.lms;
        (
            Stage::DacCleanup,
            Self::script(&[
                lms.write(ConverterEnables::RUNNING),
                lms.write(lms.clock_enables),
            ]),
        )
    }

    /// Bandwidth code `code`'s LPF tuning, with the transmitter on
    /// (`lms6002d_trf_enable(true)`); after code 15, the TX filter's DC offsets.
    fn lpf(&mut self, code: u8) -> (Stage, Part) {
        if code == 16 {
            return self.filter_setup(Lpf::Tx);
        }
        let tx_was_on = self.lms.enable_config.tx_enabled();
        self.lms.set_pll_modulator_clock(Pll::Tx, true);
        self.lms.enable_config.set_tx_enabled(true);
        self.lpf_failed = false;
        (
            Stage::Lpf {
                code,
                step: LpfStep::Enables,
                tx_was_on,
            },
            Part::Script(self.lms.enables()),
        )
    }

    /// The LPF tuning's step after `step`.
    fn lpf_step(&mut self, code: u8, step: LpfStep, tx_was_on: bool) -> (Stage, Part) {
        let tuning = LpfTuning::new().with_bandwidth(u4::new(code));
        let lms = self.lms;
        let at = |step| Stage::Lpf {
            code,
            step,
            tx_was_on,
        };
        // libusdr chains its steps on the first error but still waits, and still restores
        // the transmitter's cached state, after one.
        let next = match step {
            LpfStep::Enables if code == 0 && !self.lpf_failed => LpfStep::Tune,
            LpfStep::Enables | LpfStep::Tune if !self.lpf_failed => LpfStep::Engine,
            LpfStep::Enables | LpfStep::Tune | LpfStep::Engine => LpfStep::Wait,
            LpfStep::Wait if !self.lpf_failed => LpfStep::Status,
            LpfStep::Wait | LpfStep::Status => {
                if !tx_was_on {
                    self.lms.enable_config.set_tx_enabled(false);
                }
                if self.lpf_failed {
                    return (Stage::Done, Part::Script(Script::new()));
                }
                LpfStep::Close
            }
            LpfStep::Close => return self.lpf(code + 1),
        };
        let part = match next {
            LpfStep::Tune => Part::Tune(PllTune::new(
                lms,
                Pll::Tx,
                LPF_TUNING_TX_LO_HZ,
                self.reference_hz,
            )),
            LpfStep::Engine => Self::script(&[
                lms.write(LpfTuningControl::new()),
                lms.write(tuning),
                lms.write(tuning.with_enabled(true)),
                lms.write(LpfTuningControl::new().with_reset(true)),
                lms.write(LpfTuningControl::new()),
            ]),
            LpfStep::Wait => Part::Script(Script::new().sleep(Duration::from_micros(10))),
            LpfStep::Status => Self::script(&[lms.read_at::<DcStatus>(DcBlock::Lpf)]),
            LpfStep::Close => {
                let lms = self.lms;
                Self::script(&[
                    lms.write(LpfTuningControl::new().with_clock_off(true)),
                    lms.write(tuning),
                    lms.write(lms.enable_config),
                ])
            }
            LpfStep::Enables => unreachable!("invariant: no step leads back to the enables"),
        };
        (at(next), part)
    }

    /// A filter's DC offsets (`lms6002d_cal_txrxlpfdc`): its calibration clock on.
    fn filter_setup(&mut self, lpf: Lpf) -> (Stage, Part) {
        self.lms
            .clock_enables
            .set_rx_lpf_dc_calibration_clock(lpf == Lpf::Rx);
        self.lms
            .clock_enables
            .set_tx_lpf_dc_calibration_clock(lpf == Lpf::Tx);
        let lms = self.lms;
        (
            Stage::FilterSetup(lpf),
            Self::script(&[lms.write(lms.clock_enables)]),
        )
    }

    /// A filter's cleanup: every calibration clock off.
    fn filter_cleanup(&mut self, lpf: Lpf) -> (Stage, Part) {
        let clocks = &mut self.lms.clock_enables;
        clocks.set_lpf_calibration_clock(false);
        clocks.set_rx_lpf_dc_calibration_clock(false);
        clocks.set_tx_lpf_dc_calibration_clock(false);
        let lms = self.lms;
        (
            Stage::FilterCleanup(lpf),
            Self::script(&[lms.write(lms.clock_enables)]),
        )
    }

    /// RXVGA2's cleanup: comparators off, its clock off, and libusdr's whole setup array
    /// again, whose third word is still stage A's gain.
    fn vga2_cleanup(&mut self) -> (Stage, Part) {
        self.lms
            .clock_enables
            .set_rx_vga2_dc_calibration_clock(false);
        let lms = self.lms;
        (
            Stage::Vga2Cleanup,
            Self::script(&[
                lms.write(
                    rx_vga2::CalibrationPower::new()
                        .with_b_off(true)
                        .with_a_off(true),
                ),
                lms.write(lms.clock_enables),
                lms.write(stage_a()),
            ]),
        )
    }

    /// Terminates the RX mixer's external input and routes the mixer to the pads, or
    /// undoes both (`lms6002d_set_rx_extterm`). libusdr also zeroes the I-channel DC trim
    /// and the mixer's LO bias (its reset value is 3).
    fn termination(&self, on: bool) -> Part {
        Self::script(&[
            self.lms.write(MixerInput::new().with_from_lna(!on)),
            self.lms.write(MixerBias::new().with_input_terminated(on)),
        ])
    }
}

/// RXVGA2's stage A at 10, which its calibration runs with.
fn stage_a() -> rx_vga2::Gain {
    rx_vga2::Gain::new().with_a(u4::new(10))
}

/// The DC engine of `lpf`.
const fn filter_block(lpf: Lpf) -> DcBlock {
    match lpf {
        Lpf::Tx => DcBlock::TxLpf,
        Lpf::Rx => DcBlock::RxLpf,
    }
}

impl Sequence for RxDcCalibration {
    fn next(&self) -> Option<SequenceAction> {
        if self.stage == Stage::Done {
            return None;
        }
        self.part.next()
    }

    fn attempts_after_failure(&self) -> bool {
        true
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        let completed = self.part.complete(result);
        if let Part::Tune(tune) = self.part {
            self.lms = *tune.chip();
            if let Some(Err(fault)) = tune.outcome() {
                self.fault = Some(fault);
                self.lpf_failed = true;
            }
        }
        match completed {
            Err(_) if self.lenient() => {
                if self.part.next().is_none() {
                    self.advance(false);
                }
            }
            Err(failure) => {
                if matches!(self.stage, Stage::Lpf { step, .. } if step != LpfStep::Close) {
                    // The wait still follows a failed write, and the transmitter's cached
                    // state is restored after any failure before the close.
                    self.lpf_failed = true;
                    if self.part.next().is_none() {
                        self.advance(false);
                    }
                } else {
                    self.stage = Stage::Done;
                }
                self.settle();
                return Err(failure);
            }
            Ok(()) => {}
        }
        self.settle();
        Ok(())
    }
}
