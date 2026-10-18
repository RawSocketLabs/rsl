//! LO-leakage and IQ-imbalance calibration: `usdr_calibrate` in
//! `device/m2_lm6_1/usdr_ctrl.c`, as `/dm/sdr/0/calibrate` reaches it
//! (`usdr_device_m2_lm6_1_calibrate_set`), over the searches of `cal/cal_lo_iqimb.c`.
//!
//! Each calibration loops the transmitter into the receiver (the LMS6002D's RF loopback,
//! or a cable), sends a test tone from the TX chain's constant generator and NCOs, tunes
//! the other chain so the component it minimises lands on the RX NCO, and searches the
//! correction with `cal::search`'s 2-D search against the FPGA's DC estimator
//! (`fpga::dc_estimator`):
//!
//! - RX IQ imbalance: the TX LO sits a fraction of the RX rate above the RX LO; the search
//!   moves the RX IQ correction (phase, then amplitude) to null the tone's image.
//! - TX LO leakage: the RX LO sits that fraction below the TX LO, so the leakage lands on
//!   the RX NCO; the search moves the TX DC offset (or first TXVGA1's DC shift).
//! - TX IQ imbalance: the tone sits off the TX LO; the search moves the TX IQ correction
//!   to null the tone's image.
//!
//! Afterwards the PLLs, RX input, RXVGA1 and TXVGA1 gains (after the TX IQ search),
//! generator, NCOs and filters go back, and a chain with no LO goes down again. The
//! corrections found stay applied and kept, as libusdr keeps them, and so do, as in
//! libusdr: TXVGA1 at whatever gain the RX IQ search's tone levelling raised it to;
//! TXVGA1's analog DC shift from the analog LO stage; a chain that was tuned but
//! unpowered, which the calibration powers and leaves on; and the DC calibration of a
//! receiver the TX calibrations had to wake, run with its PLL at 320 MHz.
//!
//! Everything here is register access: [`Calibrate`] is a sans-I/O machine over the
//! board's other machines, which the blocking board runs ([`Board::calibrate`]) and the
//! native USB handle runs too, where a transfer failure ends the session and with it the
//! machine ([`Calibrate::abort`]), skipping whatever restore and power-down remain; it does
//! not stream.

use std::mem;
use std::time::Duration;

use bnb::{u5, u12};
use deps::tracing::debug;

use super::Board;
use super::board::Corrections;
use super::correction::TxDcOffset;
use crate::cal::search::{Axis, Best2d, Limits, Probe2d, Search, Search2d, Stage};
use crate::chips::register::run_sequence;
use crate::error::Error;
use crate::fpga::dc_estimator::DcMeasure;
use crate::fpga::{IqImbalance, Nco, Phy};
use crate::portable::fpga::FPGA;
use crate::portable::lms6002d::{Lna, PllTune, RxDcCalibration, TuneFault};
use crate::portable::radio::run::parts;
use crate::portable::radio::state::nco_word;
use crate::portable::radio::{Chain, ChainPower, Radio, Run, RxGain};
use crate::portable::script::Script;
use crate::portable::sequence::{Sequence, SequenceAction, SequenceFailure};

/// Which calibrations to run and how (libusdr's `USDR_CAL_*` flags).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent choices, each one of libusdr's flags"
)]
pub struct Calibration {
    /// Null the transmitter's LO leakage with the TX DC offset (`USDR_CAL_TXLO`). Needs a
    /// TX frequency.
    pub tx_lo_leakage: bool,

    /// Null the transmitter's IQ image with the TX IQ correction (`USDR_CAL_TXIQIMB`).
    /// Needs a TX frequency.
    pub tx_iq_imbalance: bool,

    /// Null the receiver's IQ image with the RX IQ correction (`USDR_CAL_RXIQIMB`). Needs
    /// an RX frequency.
    pub rx_iq_imbalance: bool,

    /// How the transmitter reaches the receiver.
    pub feedback: Feedback,

    /// Before the TX DC offset, search TXVGA1's analog DC shift (libusdr's
    /// `USDR_CAL_DUAL_RXLO`). As in libusdr, both searches then span only -128 to 127.
    pub analog_lo_first: bool,
}

/// How the transmitter reaches the receiver during a calibration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Feedback {
    /// The LMS6002D's RF loopback, switched on for the calibration and off after.
    #[default]
    Internal,

    /// An external path, such as a cable with attenuation; the loopback switch is left
    /// alone (`USDR_CAL_EXT_FB`).
    External,
}

/// What a calibration found; `None` for one not run, because it was not asked for or its
/// chain has no frequency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CalibrationReport {
    /// The TX DC offset now applied.
    pub tx_lo_leakage: Option<TxLoLeakage>,

    /// The TX IQ correction now applied, or why there is none.
    pub tx_iq_imbalance: Option<IqCalibration>,

    /// The RX IQ correction now applied, or why there is none.
    pub rx_iq_imbalance: Option<IqCalibration>,
}

/// The TX DC offset the LO-leakage search settled on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxLoLeakage {
    /// The I offset.
    pub i: TxDcOffset,

    /// The Q offset.
    pub q: TxDcOffset,
}

/// The outcome of an IQ-imbalance search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IqCalibration {
    /// The correction the search settled on, now applied.
    Corrected(IqImbalance),

    /// The test tone stayed below -70 dBFS even at full power and gain, so nothing was
    /// searched (libusdr's `-ENAVAIL`, which it does not report as a failure).
    SignalTooLow,
}

/// A calibration's frequencies, from the board's state (libusdr's `struct
/// calibrate_ops`).
#[derive(Clone, Copy, Debug)]
struct Plan {
    /// The RX sample rate after decimation, in Hz (`rxsamplerate`).
    rx_rate: i32,

    /// The RX LO to calibrate at, in Hz, as C's `int` (`rxfrequency`).
    rx_hz: i32,

    /// The TX LO to calibrate at, in Hz, as C's `int` (`txfrequency`).
    tx_hz: i32,
}

/// Where the RX LO sits below the TX LO for the LO-leakage search, as a fraction of the RX
/// rate in 2^-31 units (`rxtxlo_frac`); "very odd" so harmonics do not fold onto it.
const TX_LO_FRACTION: i32 = fraction(9.0187);

/// Where the test tone sits for the IQ searches (`rxiqimb_frac`).
const RX_IQ_FRACTION: i32 = fraction(5.1031);

/// Where the RX LO sits below the TX LO for the TX IQ search (`txiqimb_frac`).
const TX_IQ_FRACTION: i32 = fraction(11.1076);

/// `2^31 / divisor`, truncated, as libusdr's `((uint64_t)INT_MAX + 1) / divisor`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "libusdr truncates the double into an int; the divisors keep it in range"
)]
const fn fraction(divisor: f64) -> i32 {
    (2_147_483_648.0 / divisor) as i32
}

/// The LO-leakage search's test tone: off the TX LO, in Hz (`347.123e5`).
const TX_LO_TONE_HZ: i32 = 34_712_300;

/// The LO-leakage search's test tone amplitude.
const TX_LO_TONE_POWER: u32 = 32;

/// The constant generator's largest amplitude in the IQ searches.
const TONE_MAX: u32 = 30_000;

/// Each filter opens to this many times the offset it must pass (`rxbw_factor`,
/// `txbw_factor`).
const BANDWIDTH_FACTOR: f32 = 2.45;

/// A search stops early below this cost, in thousandths of a dB (`defstop`).
const STOP_BELOW: i32 = -120_000;

/// The IQ corrections' amplitude range (`IMB_AMPL_MAX`).
const AMPLITUDE_LIMIT: i32 = 262_144;

/// The IQ corrections' phase range (`IMB_PHASE_MAX`).
const PHASE_LIMIT: i32 = 45_000;

/// The TXVGA1 gain code the TX IQ search runs at (-7 dB).
const TX_IQ_VGA1_CODE: u8 = 28;

/// The RX LO the TX calibrations tune a receiver they power up to, before its DC
/// calibration, in Hz.
const RX_WAKE_LO_HZ: u32 = 320_000_000;

/// How long a chain the calibration powers settles.
const WAKE: Duration = Duration::from_millis(500);

/// The RX DC estimator channel: libusdr's channel A.
const CHANNEL: u8 = 0;

/// The correction a search moves, X then Y (`usdr_set_corr_param`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// The TX LO leakage: X is libusdr's `CORR_PARAM_I`, Y its `CORR_PARAM_Q`, but it stores
    /// them crosswise, so X moves the Q offset and Y the I offset. `digital` writes the
    /// FPGA's DC offset; otherwise TXVGA1's DC shift.
    TxLo {
        /// The FPGA's offset rather than TXVGA1's shift (`cal_txlo_dig`).
        digital: bool,
    },

    /// The RX IQ correction: X the phase, Y the amplitude.
    RxIq,

    /// The TX IQ correction: X the phase, Y the amplitude.
    TxIq,
}

/// A search's stages: up to four, as the LO search runs (`calibrate_txlo`).
#[derive(Clone, Copy, Debug)]
struct Stages {
    /// The stages, the first `len` used.
    list: [Stage; 4],
    /// How many are used.
    len: usize,
}

impl Stages {
    /// `stages`, at most four.
    fn new(stages: &[Stage]) -> Self {
        let first = stages
            .first()
            .copied()
            .expect("invariant: every calibration search has stages");
        let mut list = [first; 4];
        list[..stages.len()].copy_from_slice(stages);
        Self {
            list,
            len: stages.len(),
        }
    }
}

impl AsRef<[Stage]> for Stages {
    fn as_ref(&self) -> &[Stage] {
        &self.list[..self.len]
    }
}

parts! {
    /// What a calibration runs, one at a time: register scripts, a chain's power, a
    /// synthesizer tuning, the receiver's DC calibration and the DC estimator's
    /// measurements, each the machine the board's other operations run.
    #[derive(Debug)]
    enum Part {
        /// Straight-line register work: corrections, gains, filters, NCOs, the test tone,
        /// loopback and input switches, and the waits.
        Script(Script),
        /// A chain powered on or off (`_usdr_pwr_state`).
        Power(Run<ChainPower>),
        /// A synthesizer tuned (`lms6002d_tune_pll`).
        Tune(PllTune),
        /// The receiver's DC calibration (`usdr_calib_dc`).
        RxDc(RxDcCalibration),
        /// One power measurement (`phy_do_meas_nco_avg`).
        Measure(DcMeasure),
    }
}

/// What the part that just finished found, for the step after it.
#[derive(Clone, Copy, Debug)]
enum Ended {
    /// It wrote what it had to, or nothing has run yet.
    Done,
    /// A measurement, in thousandths of a dB.
    Measured(i32),
    /// A tuning's outcome.
    Tuned(Result<(), TuneFault>),
}

/// The next thing a flow (one of the calibration's searches) wants.
#[expect(
    clippy::large_enum_variant,
    reason = "returned straight into the running part; boxing would allocate per action"
)]
enum Flow<T> {
    /// Run this part, then call the flow again with what it found.
    Part(Part),
    /// The flow finished with this.
    Done(T),
    /// A fault ends the calibration.
    Fault(CalibrationFault),
}

/// What every flow reads and changes: the board's RF state and corrections, and the
/// calibration's frequencies.
#[derive(Clone, Copy, Debug)]
struct Context {
    /// The board's RF state.
    radio: Radio,
    /// The DSP chains' corrections.
    corrections: Corrections,
    /// The calibration's frequencies.
    plan: Plan,
}

impl Context {
    /// Tunes `chain`'s synthesizer to `hz`, a C `int` as libusdr passes it
    /// (`CORR_OP_SET_FREQ`).
    fn tune(&self, chain: Chain, hz: i32) -> Part {
        let lo = u32::from_ne_bytes(hz.to_ne_bytes());
        Part::Tune(self.radio.tune_pll(chain, lo))
    }

    /// Opens `chain`'s filter to [`BANDWIDTH_FACTOR`] times `hz`, in libusdr's float
    /// arithmetic (`CORR_OP_SET_BW`).
    fn bandwidth(&self, chain: Chain, hz: i32) -> Part {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            reason = "libusdr multiplies the int by a float and passes the product as an int"
        )]
        let bandwidth = (hz as f32 * BANDWIDTH_FACTOR) as i32 as u32;
        Part::Script(match chain {
            Chain::Rx => self.radio.lms.set_rx_bandwidth(bandwidth),
            Chain::Tx => self.radio.lms.set_tx_bandwidth(bandwidth),
        })
    }

    /// Points the RX NCO of the estimator's channel at `offset_hz`.
    fn rx_nco(&self, offset_hz: i32) -> Part {
        let word = nco_word(offset_hz, self.radio.adc_rate_hz);
        let script = Phy::Rx
            .nco(Nco::Nco0, word)
            .into_iter()
            .fold(Script::new(), |script, access| script.fpga(FPGA, access));
        Part::Script(script)
    }

    /// Sends the constant generator's I at `power` (at most 30000), Q 0, shifted by both TX
    /// NCOs to `offset_hz` (`usdr_tx_gen_set`, `_usdr_set_nco`).
    fn test_tone(&self, offset_hz: i32, power: u32) -> Part {
        let level = i16::try_from(power.min(TONE_MAX)).expect("invariant: at most 30000");
        let constant = Phy::set_tx_constant((power != 0).then_some([level, 0]));
        let word = nco_word(offset_hz, self.radio.adc_rate_hz);
        let script = [Nco::Nco0, Nco::Nco1]
            .into_iter()
            .flat_map(|nco| Phy::Tx.nco(nco, word))
            .fold(constant, |script, access| script.fpga(FPGA, access));
        Part::Script(script)
    }

    /// Sets RXVGA1 `step` dB above the caller's gain, or TXVGA1 to code `10 + step`
    /// (`CORR_OP_SET_GAIN`); `None` past the stage's maximum (libusdr's `-E2BIG`), which
    /// writes nothing.
    fn raise_gain(&mut self, chain: Chain, step: u8) -> Option<Part> {
        match chain {
            Chain::Rx => {
                let db = i32::from(self.radio.rx.vga1_db) + i32::from(step) + RxGain::VGA1_MIN_DB;
                (db <= RxGain::VGA1_MAX_DB)
                    .then(|| Part::Script(self.radio.lms.set_rx_vga1_gain(db)))
            }
            Chain::Tx => (step <= 21)
                .then(|| Part::Script(self.radio.lms.set_tx_vga1_code(u5::new(10 + step)))),
        }
    }

    /// Moves `target`'s `axis` to `value` (`usdr_set_corr_param`).
    fn set_correction(&mut self, target: Target, axis: Axis, value: i32) -> Part {
        Part::Script(match target {
            Target::TxLo { digital } => {
                // libusdr stores `CORR_PARAM_Q` (Y) as I and `CORR_PARAM_I` (X) as Q, each
                // narrowed to `int16_t`.
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "libusdr narrows to int16_t; searched values fit"
                )]
                let value = value as i16;
                match axis {
                    Axis::X => self.corrections.tx_lo_dc[1] = value,
                    Axis::Y => self.corrections.tx_lo_dc[0] = value,
                }
                let [i, q] = self.corrections.tx_lo_dc;
                if digital {
                    let bits = |v: i16| u12::new(u16::from_ne_bytes(v.to_ne_bytes()) & 0xfff);
                    Phy::set_tx_dc_offset(bits(i), bits(q))
                } else {
                    // `usdr_tx_vga1_dccorr`: narrowed to `int8_t`, then offset binary.
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "libusdr narrows to int8_t and adds 0x80 in eight bits"
                    )]
                    let code = |v: i16| (v as i8 as u8).wrapping_add(0x80);
                    self.radio.lms.set_tx_vga1_dc(code(i), code(q))
                }
            }
            Target::RxIq => {
                self.corrections.rx.set(axis, value);
                Phy::Rx.set_iq_correction(self.corrections.rx.rx_terms())
            }
            Target::TxIq => {
                self.corrections.tx.set(axis, value);
                Phy::Tx.set_iq_correction(self.corrections.tx.tx_terms())
            }
        })
    }
}

/// What a [`SearchFlow`] waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Awaiting {
    /// Nothing: the search has not begun.
    Start,
    /// A set's writes.
    Set,
    /// An evaluation's writes, before its measurement.
    EvaluateSet,
    /// An evaluation's measurement.
    Measure,
}

/// A search over `target` on the board (`find_best_2d` with the calibration's
/// evaluators): each set writes the correction, each evaluation writes it and measures.
#[derive(Clone, Copy, Debug)]
struct SearchFlow {
    /// The search.
    search: Search2d<Stages>,
    /// What it moves.
    target: Target,
    /// What it waits for.
    awaiting: Awaiting,
}

impl SearchFlow {
    /// A search over `stages` moving `target`. libusdr's coarse modes, which would leave
    /// out the last stages, never apply: `usdrcal_init_calibrate` zeroes the mode after
    /// `usdr_calibrate` sets it.
    fn new(stages: &[Stage], target: Target) -> Self {
        Self {
            search: Search2d::new(Stages::new(stages), STOP_BELOW),
            target,
            awaiting: Awaiting::Start,
        }
    }

    /// The search's next part, given how the last ended; its best point once finished.
    fn next(&mut self, ctx: &mut Context, ended: Ended) -> Flow<Best2d> {
        match (self.awaiting, ended) {
            (Awaiting::Set, _) => self.search.report(0),
            (Awaiting::EvaluateSet, _) => {
                self.awaiting = Awaiting::Measure;
                return Flow::Part(Part::Measure(DcMeasure::new(CHANNEL)));
            }
            (Awaiting::Measure, Ended::Measured(cost)) => self.search.report(cost),
            (Awaiting::Start | Awaiting::Measure, _) => {}
        }
        match self.search.probe() {
            None => Flow::Done(self.search.best()),
            Some(Probe2d::Set { axis, value, .. }) => {
                self.awaiting = Awaiting::Set;
                Flow::Part(ctx.set_correction(self.target, axis, value))
            }
            Some(Probe2d::Evaluate { axis, value, .. }) => {
                self.awaiting = Awaiting::EvaluateSet;
                Flow::Part(ctx.set_correction(self.target, axis, value))
            }
        }
    }
}

/// Where a [`LevelFlow`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    /// The tone at the current amplitude is being set.
    ToneSet,
    /// It is being measured.
    ToneMeasure,
    /// The other chain's gain is being raised by `step`.
    GainSet(u8),
    /// It is being measured.
    GainMeasure(u8),
}

/// Raises the tone until it measures above -7 dBFS, then, if it is still under -10 dBFS,
/// `boost`'s VGA1 gain (the chain not being calibrated) until it is above or at the
/// stage's maximum (`_calibrate_txpwr`). Finishes with the level, or `None` under
/// -70 dBFS.
#[derive(Clone, Copy, Debug)]
struct LevelFlow {
    /// The tone's offset, in Hz.
    tone_hz: i32,
    /// The chain whose gain makes up a weak loop.
    boost: Chain,
    /// The last level measured.
    level: i32,
    /// The tone's amplitude, plus one.
    amplitude: u32,
    /// Where it is.
    at: Level,
}

impl LevelFlow {
    /// The flow for a tone at `tone_hz`, boosting `boost`; its first part.
    fn start(ctx: &Context, tone_hz: i32, boost: Chain) -> (Self, Part) {
        let flow = Self {
            tone_hz,
            boost,
            level: -120_000,
            amplitude: 128,
            at: Level::ToneSet,
        };
        (flow, ctx.test_tone(tone_hz, flow.amplitude - 1))
    }

    /// The next part, given how the last ended; the level once finished.
    fn next(&mut self, ctx: &mut Context, ended: Ended) -> Flow<Option<i32>> {
        let measured = match ended {
            Ended::Measured(level) => level,
            Ended::Done | Ended::Tuned(_) => self.level,
        };
        match self.at {
            Level::ToneSet => {
                self.at = Level::ToneMeasure;
                return Flow::Part(Part::Measure(DcMeasure::new(CHANNEL)));
            }
            Level::ToneMeasure => {
                self.level = measured;
                if self.level <= -7000 {
                    self.amplitude <<= 1;
                    if self.amplitude <= 32_768 {
                        self.at = Level::ToneSet;
                        return Flow::Part(ctx.test_tone(self.tone_hz, self.amplitude - 1));
                    }
                }
                if self.level < -10_000 {
                    return self.raise(ctx, 0);
                }
            }
            Level::GainSet(step) => {
                self.at = Level::GainMeasure(step);
                return Flow::Part(Part::Measure(DcMeasure::new(CHANNEL)));
            }
            Level::GainMeasure(step) => {
                self.level = measured;
                if self.level <= -10_000 {
                    return self.raise(ctx, step + 1);
                }
            }
        }
        self.finish()
    }

    /// Raises the boost chain's gain by `step`, if it has room and steps remain.
    fn raise(&mut self, ctx: &mut Context, step: u8) -> Flow<Option<i32>> {
        if step >= 32 {
            return self.finish();
        }
        match ctx.raise_gain(self.boost, step) {
            Some(part) => {
                self.at = Level::GainSet(step);
                Flow::Part(part)
            }
            None => self.finish(),
        }
    }

    /// The level, or `None` under -70 dBFS.
    fn finish(&self) -> Flow<Option<i32>> {
        debug!(level = self.level, "calibration tone levelled");
        Flow::Done((self.level >= -70_000).then_some(self.level))
    }
}

/// Where an [`IqFlow`] is.
#[derive(Clone, Copy, Debug)]
enum Iq {
    /// The other chain is being tuned.
    Tune,
    /// The RX filter is being opened.
    RxBandwidth,
    /// The TX filter is being opened.
    TxBandwidth,
    /// The RX NCO is being pointed at the tone.
    SignalNco,
    /// The tone is being levelled.
    Level(LevelFlow),
    /// The RX NCO is being pointed at the image.
    ImageNco,
    /// The image is being measured.
    Image,
    /// The correction is being searched.
    Search(SearchFlow),
}

/// An IQ-imbalance search (`calibrate_rxiqimb`, `calibrate_txiqimb`, then
/// `_calibrate_iqimb_generic`): tune the other chain, open the filters, level the tone
/// with the RX NCO on it, then search with the NCO on its image.
#[derive(Clone, Copy, Debug)]
struct IqFlow {
    /// What it corrects.
    target: Target,
    /// The test tone's offset, in Hz.
    tone_hz: i32,
    /// The image's offset, in Hz.
    image_hz: i32,
    /// The tone's offset at the receiver, in Hz.
    signal_hz: i32,
    /// The tone's level.
    signal: i32,
    /// Where it is.
    at: Iq,
}

impl IqFlow {
    /// The RX IQ search (`calibrate_rxiqimb`): the TX LO a fifth of the RX rate above the
    /// RX LO, the tone on it, its image nulled; its first part.
    fn rx(ctx: &Context) -> (Self, Part) {
        let offset = ctx.plan.offset(RX_IQ_FRACTION);
        let tune = ctx.tune(Chain::Tx, ctx.plan.rx_hz.wrapping_add(offset));
        (Self::new(Target::RxIq, 0, offset, -offset), tune)
    }

    /// The TX IQ search (`calibrate_txiqimb`): the RX LO a ninth of the RX rate below the
    /// TX LO, the tone a fifth above it, its image nulled; its first part.
    fn tx(ctx: &Context) -> (Self, Part) {
        let below = ctx.plan.offset(TX_IQ_FRACTION);
        let tone = ctx.plan.offset(RX_IQ_FRACTION);
        let retune = ctx.tune(Chain::Rx, ctx.plan.tx_hz.wrapping_sub(below));
        (
            Self::new(Target::TxIq, tone, tone - below, -below - tone),
            retune,
        )
    }

    /// The search for `target` once its tuning ran.
    const fn new(target: Target, tone_hz: i32, image_hz: i32, signal_hz: i32) -> Self {
        Self {
            target,
            tone_hz,
            image_hz,
            signal_hz,
            signal: 0,
            at: Iq::Tune,
        }
    }

    /// The next part, given how the last ended; the outcome once finished.
    fn next(&mut self, ctx: &mut Context, ended: Ended) -> Flow<IqCalibration> {
        match &mut self.at {
            Iq::Tune => {
                if let Ended::Tuned(Err(fault)) = ended {
                    return Flow::Fault(CalibrationFault::Tune(fault));
                }
                let needed = self.signal_hz.abs().max(self.image_hz.abs());
                self.at = Iq::RxBandwidth;
                Flow::Part(ctx.bandwidth(Chain::Rx, needed))
            }
            Iq::RxBandwidth => {
                self.at = Iq::TxBandwidth;
                Flow::Part(ctx.bandwidth(Chain::Tx, self.tone_hz.abs()))
            }
            Iq::TxBandwidth => {
                self.at = Iq::SignalNco;
                Flow::Part(ctx.rx_nco(self.signal_hz))
            }
            Iq::SignalNco => {
                // The other chain's gain makes up a weak loop.
                let boost = if self.target == Target::TxIq {
                    Chain::Rx
                } else {
                    Chain::Tx
                };
                let (level, part) = LevelFlow::start(ctx, self.tone_hz, boost);
                self.at = Iq::Level(level);
                Flow::Part(part)
            }
            Iq::Level(level) => match level.next(ctx, ended) {
                Flow::Part(part) => Flow::Part(part),
                Flow::Fault(error) => Flow::Fault(error),
                Flow::Done(None) => Flow::Done(IqCalibration::SignalTooLow),
                Flow::Done(Some(signal)) => {
                    self.signal = signal;
                    self.at = Iq::ImageNco;
                    Flow::Part(ctx.rx_nco(self.image_hz))
                }
            },
            Iq::ImageNco => {
                self.at = Iq::Image;
                Flow::Part(Part::Measure(DcMeasure::new(CHANNEL)))
            }
            Iq::Image => {
                let image = match ended {
                    Ended::Measured(image) => image,
                    Ended::Done | Ended::Tuned(_) => 0,
                };
                debug!(signal = self.signal, image, "IQ image measured");
                let full = [
                    Limits {
                        min: -PHASE_LIMIT,
                        max: PHASE_LIMIT,
                    },
                    Limits {
                        min: -AMPLITUDE_LIMIT,
                        max: AMPLITUDE_LIMIT,
                    },
                ];
                let fine = Limits { min: -8, max: 8 };
                let golden = Stage {
                    search: Search::Golden,
                    limits: full,
                    extra_step: 0,
                };
                let stages = [
                    golden,
                    golden,
                    Stage {
                        search: Search::Iterate,
                        limits: [fine, fine],
                        extra_step: 0,
                    },
                ];
                let mut search = SearchFlow::new(&stages, self.target);
                let flow = search.next(ctx, Ended::Done);
                self.at = Iq::Search(search);
                self.searched(flow)
            }
            Iq::Search(search) => {
                let flow = search.next(ctx, ended);
                self.searched(flow)
            }
        }
    }

    /// The search's next part, or the correction it settled on.
    fn searched(&self, flow: Flow<Best2d>) -> Flow<IqCalibration> {
        match flow {
            Flow::Part(part) => Flow::Part(part),
            Flow::Fault(error) => Flow::Fault(error),
            Flow::Done(best) => {
                debug!(target = ?self.target, ?best, "IQ imbalance calibrated");
                Flow::Done(IqCalibration::Corrected(IqImbalance {
                    amplitude: best.y,
                    phase: best.x,
                }))
            }
        }
    }
}

/// Where a [`LoFlow`] is.
#[derive(Clone, Copy, Debug)]
enum Lo {
    /// The test tone is being set.
    Tone,
    /// The receiver is being tuned below the transmitter.
    Tune,
    /// The RX filter is being opened.
    Bandwidth,
    /// The RX NCO is being pointed at the leakage.
    Nco,
    /// The offset is being searched.
    Search(SearchFlow),
}

/// One LO-leakage search within `limit` (`calibrate_txlo`'s body): the test tone, the RX
/// LO a fraction of the RX rate below the TX LO, the leakage on the RX NCO, then the
/// search over the FPGA's DC offset (`digital`) or TXVGA1's DC shift.
#[derive(Clone, Copy, Debug)]
struct LoFlow {
    /// The search's absolute limits.
    limit: Limits,
    /// The FPGA's offset rather than TXVGA1's shift.
    digital: bool,
    /// Where it is.
    at: Lo,
}

impl LoFlow {
    /// The search within `limit`; its first part.
    fn start(ctx: &Context, limit: Limits, digital: bool) -> (Self, Part) {
        let flow = Self {
            limit,
            digital,
            at: Lo::Tone,
        };
        (flow, ctx.test_tone(TX_LO_TONE_HZ, TX_LO_TONE_POWER))
    }

    /// The next part, given how the last ended; the best point once finished.
    fn next(&mut self, ctx: &mut Context, ended: Ended) -> Flow<Best2d> {
        let offset = ctx.plan.offset(TX_LO_FRACTION);
        match &mut self.at {
            Lo::Tone => {
                self.at = Lo::Tune;
                Flow::Part(ctx.tune(Chain::Rx, ctx.plan.tx_hz.wrapping_sub(offset)))
            }
            Lo::Tune => {
                if let Ended::Tuned(Err(fault)) = ended {
                    return Flow::Fault(CalibrationFault::Tune(fault));
                }
                self.at = Lo::Bandwidth;
                Flow::Part(ctx.bandwidth(Chain::Rx, offset.abs()))
            }
            Lo::Bandwidth => {
                self.at = Lo::Nco;
                Flow::Part(ctx.rx_nco(-offset))
            }
            Lo::Nco => {
                let limit = self.limit;
                let eighth = Limits {
                    min: limit.min / 8,
                    max: limit.max / 8,
                };
                let stage = |search, limits: Limits, extra_step| Stage {
                    search,
                    limits: [limits; 2],
                    extra_step,
                };
                let stages = [
                    stage(Search::Golden, limit, 0),
                    stage(Search::Golden, eighth, 0),
                    stage(Search::Iterate, Limits { min: -80, max: 80 }, 4),
                    // libusdr measures four times longer here, but its estimator ignores
                    // the duration.
                    stage(Search::Iterate, Limits { min: -8, max: 8 }, 0),
                ];
                let target = Target::TxLo {
                    digital: self.digital,
                };
                let mut search = SearchFlow::new(&stages, target);
                let flow = search.next(ctx, Ended::Done);
                self.at = Lo::Search(search);
                flow
            }
            Lo::Search(search) => search.next(ctx, ended),
        }
    }
}

/// Where a [`Calibrate`] is: the steps of `usdr_calibrate` in order, each naming the part
/// that runs in it.
#[derive(Clone, Copy, Debug)]
enum Phase {
    /// Nothing has run.
    Start,
    /// RX IQ: the transmitter powered for the tone.
    RxIqTxPower,
    /// RX IQ: the transmitter's wake-up wait.
    RxIqTxWake,
    /// RX IQ: the TX IQ correction written again.
    RxIqTxCorrection,
    /// RX IQ: the RF loopback on.
    RxIqLoopback,
    /// RX IQ: the search.
    RxIq(IqFlow),
    /// RX IQ: the TX PLL tuned back.
    RxIqTxRetune,
    /// RX IQ: the TX PLL turned off.
    RxIqTxPllOff,
    /// RX IQ: the RF loopback off.
    RxIqLoopbackOff,
    /// TX: the receiver powered.
    TxRxPower,
    /// TX: the receiver's wake-up wait.
    TxRxWake,
    /// TX: the receiver tuned to [`RX_WAKE_LO_HZ`].
    TxRxTune,
    /// TX: the receiver's DC calibration.
    TxRxDc,
    /// TX: the RX IQ correction written again.
    TxRxCorrection,
    /// TX: the TX PLL tuned to the calibration's LO.
    TxRetune,
    /// TX: the receiver's input set to LNA1.
    TxLna,
    /// TX: the RF loopback on.
    TxLoopback,
    /// TX LO leakage: the analog search, if asked for, then the digital one.
    TxLo(LoFlow),
    /// TX LO leakage: done, or not asked for.
    TxLoDone,
    /// TX IQ: TXVGA1 at the search's gain.
    TxIqGain,
    /// TX IQ: the search.
    TxIq(IqFlow),
    /// TX IQ: TXVGA1 back at its code.
    TxIqGainBack,
    /// TX finish: the TX PLL tuned back from its NCO offset.
    FinishTxRetune,
    /// TX finish: the RX PLL tuned back.
    FinishRxRetune,
    /// TX finish: the RX PLL turned off.
    FinishRxPllOff,
    /// TX finish: the receiver's input restored.
    FinishLna,
    /// TX finish: the RF loopback off.
    FinishLoopbackOff,
    /// TX finish: RXVGA1 back at the caller's gain.
    FinishRxGain,
    /// Restore: the constant generator off.
    RestoreGenerator,
    /// Restore: the TX NCOs.
    RestoreTxNcos,
    /// Restore: the RX NCOs.
    RestoreRxNcos,
    /// Restore: the RX filter.
    RestoreRxBandwidth,
    /// Restore: the TX filter.
    RestoreTxBandwidth,
    /// The receiver powered down again, as it was.
    RxPowerDown,
    /// The transmitter powered down again, as it was.
    TxPowerDown,
    /// Finished.
    Done,
}

/// libusdr's LO-leakage and IQ-imbalance calibration (`usdr_calibrate`) as a sans-I/O
/// machine over the board's other machines ([`Part`]): the blocking board and the native
/// USB handle run it the same way, lending the board's RF state and corrections and taking
/// them back ([`Self::finish`]). Its arithmetic (the IQ terms' trigonometry, the power's
/// logarithm) needs the host's libm, so it lives outside the portable tree.
///
/// A bus failure ends it, except while restoring the board, where the power-downs that
/// follow are still attempted, and in those power-downs, whose failures are ignored, as
/// libusdr ignores them. A tuning or measurement fault ends it with [`Self::finish`]'s
/// error.
#[derive(Debug)]
pub(crate) struct Calibrate {
    /// The state the flows read and change.
    ctx: Context,
    /// What to run.
    request: Calibration,
    /// The TX LO the caller set, less any NCO offset; 0 untuned.
    tx_lo: u32,
    /// The RX LO the caller set, less any NCO offset; 0 untuned.
    rx_lo: u32,
    /// The receiver's input before the calibration.
    rx_input: Lna,
    /// The TX PLL is retuned for the TX calibrations.
    retune_tx: bool,
    /// TXVGA1's code before the TX IQ search.
    tx_vga1: u5,
    /// The TX finish's PLL outcome, which decides its next step.
    restored: Result<(), TuneFault>,
    /// What it found.
    report: CalibrationReport,
    /// Where it is.
    phase: Phase,
    /// The part running.
    part: Option<Part>,
    /// The running part reported a bus failure.
    failed: bool,
    /// A bus failure has been reported.
    reported: bool,
    /// The fault that ended it.
    fault: Option<CalibrationFault>,
}

/// A fault that ends a calibration without a bus failure: the reasons of
/// [`Error::UnsupportedFrequency`], [`Error::PllUnlocked`], [`Error::PllFault`] and
/// [`Error::Timeout`] (no measurement in time), kept `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationFault {
    /// A tuning's.
    Tune(TuneFault),
    /// No measurement finished in time.
    Timeout,
}

impl From<CalibrationFault> for Error {
    fn from(fault: CalibrationFault) -> Self {
        match fault {
            CalibrationFault::Tune(fault) => fault.into(),
            CalibrationFault::Timeout => Self::Timeout,
        }
    }
}

impl Calibrate {
    /// The calibrations `request` asks for on a board with `radio` and `corrections`, in
    /// libusdr's order (RX IQ, then TX LO, then TX IQ), restoring the board's tuning and
    /// filters after.
    ///
    /// # Errors
    ///
    /// [`Error::NoSampleRate`] before any sample rate, where libusdr divides by zero.
    pub(crate) fn new(
        radio: Radio,
        corrections: Corrections,
        request: Calibration,
    ) -> Result<Self, Error> {
        let decimation = radio.rx.decimation.ok_or(Error::NoSampleRate)?;
        let lowered = |lo: u32, offset: i32| {
            if lo == 0 {
                0
            } else {
                lo.wrapping_sub(u32::from_ne_bytes(offset.to_ne_bytes()))
            }
        };
        let tx_lo = lowered(radio.tx.lo_hz, radio.tx.lo_offset_hz);
        let rx_lo = lowered(radio.rx.pll_lo_hz, radio.rx.lo_offset_hz);
        let rate = radio.adc_rate_hz / decimation.factor();
        let as_int = |hz: u32| i32::from_ne_bytes(hz.to_ne_bytes());
        let mut plan = Plan {
            rx_rate: as_int(rate),
            rx_hz: as_int(rx_lo),
            tx_hz: as_int(tx_lo),
        };
        if radio.tx.lo_offset_hz != 0 {
            // Extend the measurement to fit RX: libusdr's `int += double`, with its TX
            // rate (`dac_clk / txbb_intr`), which equals the RX rate on the uSDR.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "libusdr converts the double sum back to int"
            )]
            let extended = (f64::from(plan.tx_hz) + 0.2 * f64::from(plan.rx_rate)) as i32;
            plan.tx_hz = extended;
        }
        let mut calibrate = Self {
            ctx: Context {
                radio,
                corrections,
                plan,
            },
            request,
            tx_lo,
            rx_lo,
            rx_input: radio.lms.active_lna(),
            retune_tx: true,
            tx_vga1: radio.lms.tx_vga1_code(),
            restored: Ok(()),
            report: CalibrationReport::default(),
            phase: Phase::Start,
            part: None,
            failed: false,
            reported: false,
            fault: None,
        };
        calibrate.part = calibrate.advance(Ended::Done);
        calibrate.settle();
        Ok(calibrate)
    }

    /// The board's RF state and corrections as the calibration has left them, and its
    /// report, or the fault that ended it (a bus failure is the executor's to report).
    pub(crate) fn finish(&self) -> (Radio, Corrections, Result<CalibrationReport, Error>) {
        let result = self
            .fault
            .map_or(Ok(self.report), |fault| Err(fault.into()));
        (self.ctx.radio, self.ctx.corrections, result)
    }

    /// The RF loopback is the calibration's to switch (internal feedback).
    fn internal(&self) -> bool {
        self.request.feedback == Feedback::Internal
    }

    /// Whether the current phase ignores bus failures: the closing power-downs.
    const fn ignoring(&self) -> bool {
        matches!(self.phase, Phase::RxPowerDown | Phase::TxPowerDown)
    }

    /// Whether the current phase restores the board, where a bus failure skips to the
    /// power-downs.
    const fn restoring(&self) -> bool {
        matches!(
            self.phase,
            Phase::RestoreGenerator
                | Phase::RestoreTxNcos
                | Phase::RestoreRxNcos
                | Phase::RestoreRxBandwidth
                | Phase::RestoreTxBandwidth
        )
    }

    /// Takes back what `part` changed and what it found; a measurement that timed out or
    /// a DC calibration's tuning fault becomes the calibration's fault.
    fn absorb(&mut self, part: &Part) -> Ended {
        match part {
            Part::Script(_) => Ended::Done,
            Part::Power(run) => {
                self.ctx.radio = run.radio();
                Ended::Done
            }
            Part::Tune(tune) => {
                self.ctx.radio.lms = *tune.chip();
                tune.outcome()
                    .map_or(Ended::Done, |outcome| Ended::Tuned(outcome.map(drop)))
            }
            Part::RxDc(calibration) => {
                self.ctx.radio.lms = *calibration.chip();
                if let Some(fault) = calibration.fault() {
                    self.fault = Some(CalibrationFault::Tune(fault));
                }
                Ended::Done
            }
            Part::Measure(measure) => match measure.value() {
                Some(Ok(level)) => Ended::Measured(level),
                Some(Err(())) => {
                    self.fault = Some(CalibrationFault::Timeout);
                    Ended::Done
                }
                None => Ended::Done,
            },
        }
    }

    /// Moves past finished parts until one has an action, or the calibration has ended.
    fn settle(&mut self) {
        while let Some(part) = &self.part {
            let ended = self.failed && !part.attempts_after_failure();
            if !ended && part.next().is_some() {
                return;
            }
            let part = self.part.take().expect("invariant: checked above");
            let failed = mem::take(&mut self.failed);
            let found = self.absorb(&part);
            if self.fault.is_some() {
                self.phase = Phase::Done;
                return;
            }
            let found = match (failed, self.ignoring(), self.restoring()) {
                (false, ..) | (true, true, _) => found,
                // A failed restore still powers down what the calibration woke.
                (true, false, true) => {
                    self.phase = Phase::RestoreTxBandwidth;
                    Ended::Done
                }
                (true, false, false) => {
                    self.phase = Phase::Done;
                    return;
                }
            };
            self.part = self.advance(found);
        }
    }

    /// The part after the current phase's, given how it ended; `None` once finished.
    #[expect(
        clippy::too_many_lines,
        reason = "usdr_calibrate's steps in order, one arm each, as the blocking code read"
    )]
    fn advance(&mut self, mut ended: Ended) -> Option<Part> {
        let request = self.request;
        let (tx_lo, rx_lo) = (self.tx_lo, self.rx_lo);
        loop {
            let radio = self.ctx.radio;
            let (phase, part) = match self.phase {
                Phase::Start => {
                    if !(request.rx_iq_imbalance && rx_lo > 0) {
                        self.phase = Phase::RxIqLoopbackOff;
                        continue;
                    }
                    if tx_lo == 0 || !radio.tx.powered {
                        let power = ChainPower::new(radio, Chain::Tx, true);
                        (Phase::RxIqTxPower, Some(Part::Power(Run::new(power))))
                    } else {
                        (Phase::RxIqTxCorrection, None)
                    }
                }
                Phase::RxIqTxPower => (
                    Phase::RxIqTxWake,
                    Some(Part::Script(Script::new().sleep(WAKE))),
                ),
                Phase::RxIqTxWake => {
                    let terms = self.ctx.corrections.tx.tx_terms();
                    (
                        Phase::RxIqTxCorrection,
                        Some(Part::Script(Phy::Tx.set_iq_correction(terms))),
                    )
                }
                Phase::RxIqTxCorrection => {
                    let loopback = self.internal().then(|| radio.lms.enable_rf_loopback());
                    (Phase::RxIqLoopback, loopback.map(Part::Script))
                }
                Phase::RxIqLoopback => {
                    let (flow, part) = IqFlow::rx(&self.ctx);
                    (Phase::RxIq(flow), Some(part))
                }
                Phase::RxIq(mut flow) => match flow.next(&mut self.ctx, ended) {
                    Flow::Part(part) => (Phase::RxIq(flow), Some(part)),
                    Flow::Fault(fault) => return self.fail(fault),
                    Flow::Done(found) => {
                        self.report.rx_iq_imbalance = Some(found);
                        if tx_lo == 0 {
                            let off = self.ctx.radio.lms.disable_tx_pll();
                            (Phase::RxIqTxPllOff, Some(Part::Script(off)))
                        } else {
                            (
                                Phase::RxIqTxRetune,
                                Some(Part::Tune(radio.tune_pll(Chain::Tx, tx_lo))),
                            )
                        }
                    }
                },
                Phase::RxIqTxRetune => match ended {
                    Ended::Tuned(Err(TuneFault::UnsupportedFrequency(_))) => {
                        let off = self.ctx.radio.lms.disable_tx_pll();
                        (Phase::RxIqTxPllOff, Some(Part::Script(off)))
                    }
                    Ended::Tuned(Err(fault @ TuneFault::Fault)) => {
                        return self.fail(CalibrationFault::Tune(fault));
                    }
                    _ => (Phase::RxIqTxPllOff, None),
                },
                Phase::RxIqTxPllOff => {
                    let loopback = self.internal().then(|| radio.lms.disable_rf_loopback());
                    (Phase::RxIqLoopbackOff, loopback.map(Part::Script))
                }
                Phase::RxIqLoopbackOff => {
                    if !((request.tx_lo_leakage || request.tx_iq_imbalance) && tx_lo > 0) {
                        self.phase = Phase::FinishRxGain;
                        continue;
                    }
                    if rx_lo == 0 || !radio.rx.powered {
                        self.retune_tx = false;
                        let power = ChainPower::new(radio, Chain::Rx, true);
                        (Phase::TxRxPower, Some(Part::Power(Run::new(power))))
                    } else {
                        (Phase::TxRxCorrection, None)
                    }
                }
                Phase::TxRxPower => (
                    Phase::TxRxWake,
                    Some(Part::Script(Script::new().sleep(WAKE))),
                ),
                Phase::TxRxWake => (
                    Phase::TxRxTune,
                    Some(Part::Tune(radio.tune_pll(Chain::Rx, RX_WAKE_LO_HZ))),
                ),
                Phase::TxRxTune => {
                    if let Ended::Tuned(Err(fault)) = ended {
                        return self.fail(CalibrationFault::Tune(fault));
                    }
                    let vga2 = (radio.rx.vga2a, radio.rx.vga2b);
                    let dc = RxDcCalibration::new(radio.lms, radio.reference_hz, vga2);
                    (Phase::TxRxDc, Some(Part::RxDc(dc)))
                }
                Phase::TxRxDc => {
                    let terms = self.ctx.corrections.rx.rx_terms();
                    (
                        Phase::TxRxCorrection,
                        Some(Part::Script(Phy::Rx.set_iq_correction(terms))),
                    )
                }
                Phase::TxRxCorrection => {
                    let retune = self.retune_tx || radio.tx.lo_offset_hz != 0;
                    let lo = u32::from_ne_bytes(self.ctx.plan.tx_hz.to_ne_bytes());
                    let tune = retune.then(|| Part::Tune(radio.tune_pll(Chain::Tx, lo)));
                    (Phase::TxRetune, tune)
                }
                Phase::TxRetune => {
                    if let Ended::Tuned(Err(fault)) = ended {
                        return self.fail(CalibrationFault::Tune(fault));
                    }
                    let lna = self
                        .internal()
                        .then(|| self.ctx.radio.lms.select_lna(Lna::Lna1));
                    (Phase::TxLna, lna.map(Part::Script))
                }
                Phase::TxLna => {
                    let loopback = self.internal().then(|| radio.lms.enable_rf_loopback());
                    (Phase::TxLoopback, loopback.map(Part::Script))
                }
                Phase::TxLoopback => {
                    if !request.tx_lo_leakage {
                        self.phase = Phase::TxLoDone;
                        continue;
                    }
                    // libusdr narrows the limits for the analog search and never widens
                    // them again.
                    let (limit, digital) = if request.analog_lo_first {
                        (
                            Limits {
                                min: -128,
                                max: 127,
                            },
                            false,
                        )
                    } else {
                        let max = i32::from(TxDcOffset::MAX);
                        (Limits { min: -max, max }, true)
                    };
                    let (flow, part) = LoFlow::start(&self.ctx, limit, digital);
                    (Phase::TxLo(flow), Some(part))
                }
                Phase::TxLo(mut flow) => match flow.next(&mut self.ctx, ended) {
                    Flow::Part(part) => (Phase::TxLo(flow), Some(part)),
                    Flow::Fault(fault) => return self.fail(fault),
                    // The analog search done, the digital one within its limits.
                    Flow::Done(_) if !flow.digital => {
                        let (next, part) = LoFlow::start(&self.ctx, flow.limit, true);
                        (Phase::TxLo(next), Some(part))
                    }
                    Flow::Done(best) => {
                        let offset = |value| {
                            TxDcOffset::new(value)
                                .expect("invariant: searched within the 12-bit range")
                        };
                        let [i, q] = self.ctx.corrections.tx_lo_dc;
                        let leakage = TxLoLeakage {
                            i: offset(i),
                            q: offset(q),
                        };
                        debug!(?best, ?leakage, "TX LO leakage calibrated");
                        self.report.tx_lo_leakage = Some(leakage);
                        (Phase::TxLoDone, None)
                    }
                },
                Phase::TxLoDone => {
                    if !request.tx_iq_imbalance {
                        self.phase = Phase::TxIqGainBack;
                        continue;
                    }
                    self.tx_vga1 = self.ctx.radio.lms.tx_vga1_code();
                    let gain = self
                        .ctx
                        .radio
                        .lms
                        .set_tx_vga1_code(u5::new(TX_IQ_VGA1_CODE));
                    (Phase::TxIqGain, Some(Part::Script(gain)))
                }
                Phase::TxIqGain => {
                    let (flow, part) = IqFlow::tx(&self.ctx);
                    (Phase::TxIq(flow), Some(part))
                }
                Phase::TxIq(mut flow) => match flow.next(&mut self.ctx, ended) {
                    Flow::Part(part) => (Phase::TxIq(flow), Some(part)),
                    Flow::Fault(fault) => return self.fail(fault),
                    Flow::Done(found) => {
                        self.report.tx_iq_imbalance = Some(found);
                        let gain = self.ctx.radio.lms.set_tx_vga1_code(self.tx_vga1);
                        (Phase::TxIqGainBack, Some(Part::Script(gain)))
                    }
                },
                Phase::TxIqGainBack => {
                    let retune = radio.tx.lo_offset_hz != 0;
                    let tune = retune.then(|| Part::Tune(radio.tune_pll(Chain::Tx, tx_lo)));
                    self.restored = Ok(());
                    (Phase::FinishTxRetune, tune)
                }
                Phase::FinishTxRetune => {
                    if let Ended::Tuned(outcome) = ended {
                        self.restored = outcome;
                    }
                    if rx_lo > 0 && self.restored.is_ok() {
                        let tune = Part::Tune(radio.tune_pll(Chain::Rx, rx_lo));
                        (Phase::FinishRxRetune, Some(tune))
                    } else {
                        (Phase::FinishRxRetune, None)
                    }
                }
                Phase::FinishRxRetune => {
                    if let Ended::Tuned(outcome) = ended {
                        self.restored = outcome;
                    }
                    // As in libusdr: a PLL out of range (the TX retune's, or else the RX
                    // one's) turns the RX PLL off, and one that will not lock is not an
                    // error, since its status check sees the RX step's.
                    let off = match (rx_lo > 0, self.restored) {
                        (true, Err(TuneFault::UnsupportedFrequency(_))) | (false, Ok(())) => {
                            Some(Part::Script(self.ctx.radio.lms.disable_rx_pll()))
                        }
                        (true, Err(TuneFault::Unlocked(_)) | Ok(())) => None,
                        (_, Err(fault)) => return self.fail(CalibrationFault::Tune(fault)),
                    };
                    (Phase::FinishRxPllOff, off)
                }
                Phase::FinishRxPllOff => {
                    let lna = self
                        .internal()
                        .then(|| self.ctx.radio.lms.select_lna(self.rx_input));
                    (Phase::FinishLna, lna.map(Part::Script))
                }
                Phase::FinishLna => {
                    let loopback = self.internal().then(|| radio.lms.disable_rf_loopback());
                    (Phase::FinishLoopbackOff, loopback.map(Part::Script))
                }
                Phase::FinishLoopbackOff => {
                    let db = i32::from(radio.rx.vga1_db) + RxGain::VGA1_MIN_DB;
                    (
                        Phase::FinishRxGain,
                        Some(Part::Script(radio.lms.set_rx_vga1_gain(db))),
                    )
                }
                Phase::FinishRxGain => (
                    Phase::RestoreGenerator,
                    Some(Part::Script(Phy::set_tx_constant(None))),
                ),
                Phase::RestoreGenerator => (
                    Phase::RestoreTxNcos,
                    Some(Part::Script(radio.restore_ncos(Chain::Tx))),
                ),
                Phase::RestoreTxNcos => (
                    Phase::RestoreRxNcos,
                    Some(Part::Script(radio.restore_ncos(Chain::Rx))),
                ),
                Phase::RestoreRxNcos => {
                    let script = match radio.rx.fixed_bandwidth_hz {
                        Some(hz) => Some(radio.lms.set_rx_bandwidth(hz)),
                        None => radio.update_bandwidth(Chain::Rx),
                    };
                    (Phase::RestoreRxBandwidth, script.map(Part::Script))
                }
                Phase::RestoreRxBandwidth => {
                    let script = match radio.tx.fixed_bandwidth_hz {
                        Some(hz) => Some(radio.lms.set_tx_bandwidth(hz)),
                        None => radio.update_bandwidth(Chain::Tx),
                    };
                    (Phase::RestoreTxBandwidth, script.map(Part::Script))
                }
                // libusdr powers down what was not running even after a failed restore,
                // and ignores how that goes.
                Phase::RestoreTxBandwidth => {
                    let power = (rx_lo == 0)
                        .then(|| Part::Power(Run::new(ChainPower::new(radio, Chain::Rx, false))));
                    (Phase::RxPowerDown, power)
                }
                Phase::RxPowerDown => {
                    let power = (tx_lo == 0)
                        .then(|| Part::Power(Run::new(ChainPower::new(radio, Chain::Tx, false))));
                    (Phase::TxPowerDown, power)
                }
                Phase::TxPowerDown | Phase::Done => (Phase::Done, None),
            };
            self.phase = phase;
            ended = Ended::Done;
            match (part, phase) {
                (Some(part), _) => return Some(part),
                (None, Phase::Done) => return None,
                (None, _) => {}
            }
        }
    }

    /// Ends the calibration at a failed transfer that ends the session, before reporting
    /// it: takes back what the running part changed and found, its fault included, and
    /// runs nothing more, so [`Self::finish`] reports the fault the blocking run would.
    #[cfg(all(feature = "mio", target_os = "linux"))]
    pub(crate) fn abort(&mut self) {
        if let Some(part) = self.part.take() {
            self.absorb(&part);
        }
        self.phase = Phase::Done;
    }

    /// Ends the calibration with `fault`.
    fn fail(&mut self, fault: CalibrationFault) -> Option<Part> {
        self.fault = Some(fault);
        self.phase = Phase::Done;
        None
    }
}

impl Sequence for Calibrate {
    fn next(&self) -> Option<SequenceAction> {
        self.part.as_ref()?.next()
    }

    fn attempts_after_failure(&self) -> bool {
        true
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(part) = &mut self.part else {
            return Ok(());
        };
        let outcome = match part.complete(result) {
            Ok(()) => Ok(()),
            Err(failure) => {
                self.failed = true;
                if self.ignoring() || self.reported {
                    Ok(())
                } else {
                    self.reported = true;
                    Err(failure)
                }
            }
        };
        self.settle();
        outcome
    }
}

impl Board {
    /// Runs the calibrations `request` asks for, in libusdr's order (RX IQ, then TX LO, then
    /// TX IQ), and restores the board's tuning and filters (`usdr_calibrate`): the
    /// [`Calibrate`] machine over the bus, taking the board's RF state and corrections
    /// back.
    ///
    /// Divergences: calibrating before any sample rate fails with
    /// [`Error::NoSampleRate`] before any bus access, where libusdr divides by zero; so
    /// does calibrating with a stream group, with [`Error::AlreadyStreaming`], where
    /// libusdr would send its test tone instead of the stream's samples. The
    /// report gives each IQ search's own result, where libusdr's result array repeats the
    /// LO search's (or uninitialised memory). Channel B is not offered.
    pub(crate) fn calibrate(&mut self, request: Calibration) -> Result<CalibrationReport, Error> {
        if self.streams.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let mut calibrate = Calibrate::new(self.radio, self.corrections, request)?;
        let ran = run_sequence(self.bus.as_mut(), &mut calibrate);
        let (radio, corrections, result) = calibrate.finish();
        (self.radio, self.corrections) = (radio, corrections);
        // A fault first: the DC calibration reports its tuning fault over a later bus
        // failure, as libusdr's does.
        let report = result?;
        ran?;
        debug!(?report, "calibrated");
        Ok(report)
    }
}

impl Plan {
    /// `fraction` of the RX rate, in Hz: `(rate · fraction) >> 31`.
    fn offset(&self, fraction: i32) -> i32 {
        let offset = (i64::from(self.rx_rate) * i64::from(fraction)) >> 31;
        i32::try_from(offset).expect("invariant: a fraction below 1 of an int rate")
    }
}

impl IqImbalance {
    /// Sets the phase (X) or amplitude (Y), as libusdr's `CORR_PARAM_A` and
    /// `CORR_PARAM_GIQ` do.
    fn set(&mut self, axis: Axis, value: i32) {
        match axis {
            Axis::X => self.phase = value,
            Axis::Y => self.amplitude = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_are_libusdrs_truncated_quotients() {
        assert_eq!(TX_LO_FRACTION, 238_114_545);
        assert_eq!(RX_IQ_FRACTION, 420_819_432);
        assert_eq!(TX_IQ_FRACTION, 193_334_622);
    }

    #[test]
    fn offsets_are_the_fraction_of_the_rate_rounded_down() {
        let plan = Plan {
            rx_rate: 10_000_000,
            rx_hz: 0,
            tx_hz: 0,
        };
        assert_eq!(plan.offset(TX_LO_FRACTION), 1_108_807);
        assert_eq!(
            Plan {
                rx_rate: -10_000_000,
                ..plan
            }
            .offset(TX_LO_FRACTION),
            -1_108_808,
            "an arithmetic shift floors"
        );
    }
}
