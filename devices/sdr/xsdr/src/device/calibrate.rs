//! The RF calibration (`/dm/sdr/0/calibrate`, `xsdr_calibrate`) and the DC readback
//! (`/dm/sdr/0/rx/dccorr` read, `xsdr_rxdccorr`): libusdr's LO-leakage and I/Q-imbalance
//! searches (`cal/cal_lo_iqimb.c`, on `sdr-cal`'s `best_2d`, `cal/opt_func.c`) over the
//! XSDR's hooks (`device/m2_lm7_1/xsdr_ctrl.c`'s `xsdrcal_*`, `lms7002m_ctrl.c`'s
//! `lms7002m_set_corr_param` and `lms7002m_set_tx_testsig`), measuring with the RX PHY's
//! DC estimator.
//!
//! Each measurement holds and restarts the estimator, polls its generation every 500 µs
//! until a new one is ready (at most 8000 polls) and turns channel's I and Q into power in
//! thousandths of a dB of full scale; the calibration takes the best of two. The RX LO
//! search moves the RX DC DACs with the receiver's DC corrector bypassed; the TX LO one the
//! TX DC DACs with a DC test signal 34.7 MHz off and the receiver tuned below the TX LO; the
//! I/Q ones the processors' phase and gain corrections, with the receiver's mixer on the
//! image of a test tone whose level was first raised until it measures well, through the RF
//! loopback (or a cable). Then, unless asked not to, the paths, pad losses, mixers, mutes,
//! TX gain correction and baseband filters are restored and the test signal stopped.
//!
//! Host-side, as its power figures need `log10`; blocking code over the bus, as the
//! quality check's. `lms7002m_set_corr_param` and `lms7002m_set_tx_testsig` are
//! `lms7002m_ctrl.c` code but only this driver calls them, so they sit here.
//!
//! Divergences: a calibration before any sample rate, where libusdr divides by zero
//! (SIGFPE), is refused with [`Error::NoSampleRate`] before any bus access; the restore's
//! baseband filters are set for the rates divided by 1 off DUC/DDC gateware, where libusdr
//! divides by 0 there and traps, as in the quality check. libusdr reports the I/Q
//! calibrations' slots (`cal_data[4..8]`) from the last LO search's values, or from an
//! uninitialised stack, while this reports the corrections each found. A failed bus access
//! ends a measurement with its error, where libusdr's ends it too but may first fold a
//! stale value into its best.

use std::num::NonZeroU32;
use std::time::Duration;

use bnb::u3;
use hal::FpgaAccess;
use lms7002m::ctrl::{self, ChipCall, Direction, RfPath, RxRoute, TxRoute};
use lms7002m::{
    Channel, DcDac, Mac, RxGainStage, RxPath, Synthesizer, Tsp, TspGenerator, TuneError,
};
use lms7002m::{TxGainStage, TxPath};
use sdr_cal::search::{Axis, Limits, Objective2d, Search, Stage, best_2d};

use super::{Device, run};
use crate::error::Error;
use crate::portable::fpga::{REG_CFG_PHY_0, RxPhyBank};
use crate::portable::frontend::XsdrFrontend;
use crate::portable::step::Step;

/// The DC estimator's hold or run address (`ADDR_DC_EST_RESET`).
const ESTIMATOR_HOLD: u8 = 0;
/// Its accumulation address (`ADDR_DC_EST_ACCM`).
const ESTIMATOR_ACCUMULATION: u8 = 1;
/// Its generation word (`DC_ESTIM_GEN`).
const ESTIMATOR_GENERATION: u8 = 0;
/// Its channel A I word (`DC_ESTIM_AI`); A Q, B I and B Q follow.
const ESTIMATOR_A_I: u8 = 4;
/// Generation polls per measurement before giving up.
const ESTIMATOR_POLLS: u32 = 8000;
/// The wait between polls (`usleep(500)`).
const ESTIMATOR_POLL: Duration = Duration::from_micros(500);
/// What a measurement reports before any (`accum = -180000`), in millidB.
const NO_MEASUREMENT: i32 = -180_000;
/// The half LSB libusdr adds back to the estimator's words (`corr`).
const HALF_LSB: f32 = 0.5;

/// The cost below which a search stops (`defstop`), in millidB.
const STOP_BELOW: i32 = -140_000;
/// The TX LO search's test signal offset (`347.123e5`) and level.
const TX_LO_TONE: (i32, u32) = (34_712_300, 32);
/// The test signal's level that switches the processor back to its normal input
/// (`UINT_MAX`).
const NORMAL_INPUT: u32 = u32::MAX;
/// The filter bandwidth over the farthest offset observed (`rxbw_factor`, `txbw_factor`).
const BANDWIDTH_FACTOR: f32 = 2.33;
/// The tone's level stops rising above this power (`-7000`), in millidB.
const LOUD_ENOUGH: i32 = -7_000;
/// The TX gain correction stops rising above this power (`-10000`).
const GAIN_ENOUGH: i32 = -10_000;
/// Below this power the I/Q calibration gives up (`-70000`, libusdr's `-ENAVAIL`).
const TOO_WEAK: i32 = -70_000;

/// The RX LO search's bounds (`rxlo_iq_corr`).
const RX_LO_LIMITS: Limits = Limits { min: -63, max: 63 };
/// The TX LO search's bounds (`txlo_iq_corr`).
const TX_LO_LIMITS: Limits = Limits {
    min: -1023,
    max: 1023,
};
/// The I/Q searches' phase bounds (`rximb_ang_corr`, which the TX search uses too).
const IQ_PHASE_LIMITS: Limits = Limits {
    min: -128,
    max: 127,
};
/// Their gain bounds (`rximb_iq_corr`).
const IQ_GAIN_LIMITS: Limits = Limits {
    min: -1024,
    max: 1023,
};
/// The last stages' window, ±8.
const FINE: Limits = Limits { min: -8, max: 8 };

/// Which calibrations to run (`/dm/sdr/0/calibrate`'s `XSDR_CAL_*` flags).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "libusdr's independent flags, one each"
)]
pub struct Calibrations {
    /// The RX LO leakage (`XSDR_CAL_RXLO`), with the RX LO tuned.
    pub rx_lo: bool,

    /// The TX LO leakage (`XSDR_CAL_TXLO`), with the TX LO tuned.
    pub tx_lo: bool,

    /// The RX I/Q imbalance (`XSDR_CAL_RXIQIMB`), with the RX LO tuned.
    pub rx_iq: bool,

    /// The TX I/Q imbalance (`XSDR_CAL_TXIQIMB`), with the TX LO tuned.
    pub tx_iq: bool,

    /// A cable loops TX back to RX, so the RF loopback is not switched in
    /// (`XSDR_CAL_EXT_FB`).
    pub external_loopback: bool,

    /// Leave the calibration's settings in place instead of restoring the paths, mixers,
    /// filters and synthesizers (`XSDR_DONT_SETBACK`).
    pub keep_settings: bool,
}

/// What a calibration found: each search's best I and Q codes (the LO searches: DC DAC
/// codes) or phase and gain codes (the I/Q searches), `None` for a calibration not run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RfCalibration {
    /// The RX DC DACs' I and Q.
    pub rx_lo: Option<[i32; 2]>,

    /// The TX DC DACs' I and Q.
    pub tx_lo: Option<[i32; 2]>,

    /// The RX processor's phase code and gain balance (negative lowers Q, positive I);
    /// `None` also when, without a TX stream, the search failed and the TX stream-down
    /// after it hid the failure, as libusdr hides it.
    pub rx_iq: Option<[i32; 2]>,

    /// The TX processor's, likewise; `None` also when the test tone measured too weak,
    /// which libusdr skips (reporting 0s).
    pub tx_iq: Option<[i32; 2]>,
}

/// One of `lms7002m_set_corr_param`'s settings, in a direction.
#[derive(Clone, Copy, Debug)]
enum Corr {
    /// A DC DAC, I or Q (`CORR_PARAM_I`, `CORR_PARAM_Q`).
    Dc(Axis, i32),
    /// The processor's phase code (`CORR_PARAM_A`).
    Phase(i32),
    /// The processor's gain balance (`CORR_PARAM_GIQ`).
    GainBalance(i32),
    /// The direction's synthesizer (`CORR_OP_SET_FREQ`).
    Frequency(i32),
    /// The direction's baseband filter (`CORR_OP_SET_BW`).
    Bandwidth(i32),
}

/// The search a calibration runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The RX DC DACs.
    RxLo,
    /// The TX DC DACs.
    TxLo,
    /// The RX processor's phase and gain.
    RxIq,
    /// The TX processor's.
    TxIq,
}

/// What `xsdr_calibrate` saves before calibrating.
#[derive(Clone, Copy, Debug)]
struct Saved {
    /// The TX mutes (`dsp_txcfg`).
    mute: u8,
    /// The RX and TX paths (`rx_rfic_path`, `tx_rfic_path`).
    routes: (RxRoute, TxRoute),
    /// Each channel's TX pad loss.
    tx_loss: [u8; 2],
    /// The channel's TX gain correction (`reg_tbb_gc_corr`).
    gain_correction: i8,
    /// The channel's LNA (`rfe[ch].path`).
    lna: RxPath,
    /// The channel's TX band (`trf[ch].path`).
    band: TxPath,
    /// The RX and TX LOs.
    lo: (u32, u32),
    /// A TX channel runs.
    tx_running: bool,
}

/// The sample-rate facts the searches use (`struct calibrate_ops`, from
/// `xsdrcal_init_calibrate`).
#[derive(Clone, Copy, Debug)]
struct Plan {
    /// The RX rate after the chip's decimation (`rxsamplerate`).
    rx_rate: i32,
    /// The RX LO (`rxfrequency`, libusdr's `int`).
    rx_lo: i32,
    /// The TX LO (`txfrequency`).
    tx_lo: i32,
    /// The measurements' log duration (`deflogdur`).
    log_duration: u32,
}

impl Plan {
    /// `(rxsamplerate * fraction) >> 31` for libusdr's fraction `2^31 / divisor`.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr's double-to-int conversions and its 64-bit product shifted back"
    )]
    fn offset(&self, divisor: f64) -> i32 {
        let fraction = (2_147_483_648.0 / divisor) as i32;
        ((i64::from(self.rx_rate) * i64::from(fraction)) >> 31) as i32
    }
}

/// libusdr's fractions of the RX rate: the TX LO search's (`rxtxlo_frac`), the RX I/Q
/// search's (`rxiqimb_frac`) and the TX I/Q search's (`txiqimb_frac`) divisors.
const RX_TX_LO_DIVISOR: f64 = 23.0187;
/// See [`RX_TX_LO_DIVISOR`].
const RX_IQ_DIVISOR: f64 = 5.1031;
/// See [`RX_TX_LO_DIVISOR`].
const TX_IQ_DIVISOR: f64 = 11.1076;

impl Device {
    /// Calibrates channel `channel`'s (A or B) LO leakage and I/Q imbalance as libusdr's
    /// `/dm/sdr/0/calibrate` does (`xsdr_calibrate`), through the RF loopback (or a cable,
    /// [`Calibrations::external_loopback`]), measuring with the RX PHY's DC estimator; see
    /// the module docs. The RX calibrations need the RX LO tuned, the TX ones the TX LO and
    /// a TX band selected; the other channel's TX is muted throughout. Without a TX stream,
    /// the RX I/Q calibration streams TX up and down again around itself (with the link's
    /// calibrations), and a failure in it is then lost as libusdr loses it.
    ///
    /// Divergence: before any sample rate this refuses before any bus access, where
    /// libusdr divides by zero; off DUC/DDC gateware the restore's filters follow the rates
    /// divided by 1, where libusdr divides by 0 and traps.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NoChannel`] for other than [`Mac::A`] or [`Mac::B`]
    /// (libusdr's `-EINVAL`); [`Error::NoSampleRate`]. Then [`Error::InvalidPath`] when the
    /// channel's LNA or TX band has no RF loopback (libusdr's `-EINVAL`);
    /// [`Error::NcoOutOfRange`] for a test offset beyond half a converter clock;
    /// [`Error::TuneOutOfRange`] or [`Error::VcoComparatorFailed`] retuning;
    /// [`Error::WeakCalibrationSignal`] when the RX I/Q test tone stays too weak;
    /// [`Error::EstimatorStalled`]; a stream-up's or link calibration's errors;
    /// [`Error::Bus`].
    pub fn calibrate(
        &mut self,
        channel: Mac,
        calibrations: Calibrations,
    ) -> Result<RfCalibration, Error> {
        let index = match channel {
            Mac::A => 0,
            Mac::B => 1,
            Mac::None | Mac::Both => return Err(Error::NoChannel),
        };
        let plan = self.calibration_plan()?;
        let chip = *self.control.chip();
        let saved = Saved {
            mute: self.tx_mute,
            routes: self.control.routes(),
            tx_loss: self.control.tx_loss(),
            gain_correction: chip.tbb_gain_correction(chip_channel(channel)),
            lna: chip.rx_path(chip_channel(channel)),
            band: chip.tx_path(chip_channel(channel)),
            lo: self.control.lo(),
            tx_running: self.control.tx_running().contains(&true),
        };
        let (rx_lo, tx_lo) = saved.lo;
        let wants = calibrations;
        self.measurements = 2;
        let mut found = RfCalibration::default();

        self.run_chip(&[ChipCall::MacSet(channel)])?;
        if wants.rx_lo && rx_lo > 0 {
            self.set_rx_nco_offset(channel, 0)?;
            self.run_chip(&[ChipCall::RxtspDcCorr(true, u3::new(0))])?;
            let best = self.search(Kind::RxLo, channel, &plan)?;
            if !wants.keep_settings {
                self.run_chip(&[ChipCall::RxtspDcCorr(false, u3::new(7))])?;
            }
            found.rx_lo = Some(best);
        }
        if !(wants.tx_lo || wants.rx_iq || wants.tx_iq) && wants.keep_settings {
            return Ok(found);
        }

        self.set_tx_mute(if index == 0 { 1 } else { 2 })?;
        self.run_chip(&[ChipCall::MacSet(channel)])?;
        if wants.rx_iq && rx_lo > 0 {
            found.rx_iq = self.rx_iq_pass(channel, &plan, &saved, wants.external_loopback)?;
        }
        if (wants.tx_lo || wants.tx_iq) && tx_lo > 0 {
            self.tx_pass(channel, &plan, &saved, wants, &mut found)?;
        }
        if !wants.keep_settings {
            self.restore(channel, &saved)?;
        }
        self.set_test_signal(channel, 0, NORMAL_INPUT)?;
        Ok(found)
    }

    /// The RX I/Q calibration's part of `xsdr_calibrate`: without a TX stream, TX streamed
    /// up (and its filter set for the RX rate) first and down again after, which replaces
    /// the calibration's outcome as libusdr's `res = xsdr_prepare(..)` does; the loopback
    /// switched in; the search; then the TX synthesizer back on its LO, or off.
    fn rx_iq_pass(
        &mut self,
        channel: Mac,
        plan: &Plan,
        saved: &Saved,
        external: bool,
    ) -> Result<Option<[i32; 2]>, Error> {
        let gateware = self.gateware();
        let (_, tx_lo) = saved.lo;
        let mut attempt = || -> Result<[i32; 2], Error> {
            if !saved.tx_running {
                self.prepare(true, true, gateware)?;
                self.set_filter(Direction::Tx, plan.rx_rate.unsigned_abs())?;
                self.bus.sleep(Duration::from_millis(75));
            }
            if !external {
                self.loop_back(saved.lna, saved.band, channel, true)?;
            }
            let best = self.calibrate_rx_iq(channel, plan)?;
            if tx_lo == 0 {
                self.run_chip(&[ChipCall::SxxDisable(Synthesizer::Tx)])?;
            } else {
                self.retune(Synthesizer::Tx, tx_lo)?;
            }
            Ok(best)
        };
        let outcome = attempt();
        if saved.tx_running {
            return outcome.map(Some);
        }
        self.prepare(true, false, gateware)?;
        Ok(outcome.ok())
    }

    /// The TX calibrations' part of `xsdr_calibrate`: the loopback switched in from the
    /// TX band, the RX filter set to 1 MHz (and 750 ms) without an RX LO, the LO search,
    /// the I/Q search (a too weak tone skipping it), then unless kept the RX synthesizer
    /// back on its LO, or off.
    fn tx_pass(
        &mut self,
        channel: Mac,
        plan: &Plan,
        saved: &Saved,
        wants: Calibrations,
        found: &mut RfCalibration,
    ) -> Result<(), Error> {
        let (rx_lo, _) = saved.lo;
        if !wants.external_loopback {
            self.loop_back(saved.lna, saved.band, channel, false)?;
        }
        if rx_lo == 0 {
            self.set_filter(Direction::Rx, 1_000_000)?;
            self.bus.sleep(Duration::from_millis(750));
        }
        if wants.tx_lo {
            found.tx_lo = Some(self.calibrate_tx_lo(channel, plan)?);
        }
        if wants.tx_iq {
            found.tx_iq = match self.calibrate_tx_iq(channel, plan) {
                Err(Error::WeakCalibrationSignal) => None,
                other => Some(other?),
            };
        }
        if wants.keep_settings {
            return Ok(());
        }
        if rx_lo > 0 {
            self.retune(Synthesizer::Rx, rx_lo)
        } else {
            self.run_chip(&[ChipCall::SxxDisable(Synthesizer::Rx)])
        }
    }

    /// Measures both channels' DC power as libusdr's `/dm/sdr/0/rx/dccorr` read does
    /// (`xsdr_rxdccorr`) and returns channel A's, in thousandths of a dB of full scale:
    /// one measurement each, or the best of two once a calibration ran, as libusdr counts
    /// them. Channel A's failure reads 0, as in libusdr, which reports only B's.
    ///
    /// # Errors
    ///
    /// Channel B's measurement's: [`Error::EstimatorStalled`], [`Error::Bus`].
    pub fn rx_dc_level(&mut self) -> Result<i32, Error> {
        let a = self.measure(0, 0);
        self.measure(1, 0)?;
        Ok(a.unwrap_or(0))
    }

    /// `xsdrcal_init_calibrate`'s rate facts, refused where libusdr divides by zero.
    fn calibration_plan(&self) -> Result<Plan, Error> {
        let clocks = self.control.clocks();
        let rate = |converter: u32, tsp: u32, dsp: u32| {
            clocks
                .cgen
                .checked_div(converter)
                .and_then(|clock| clock.checked_div(tsp))
                .and_then(|rate| rate.checked_div(dsp))
        };
        // libusdr computes both directions' rates, the TX one unused.
        rate(
            clocks.tx_converter_div,
            clocks.tx_tsp_div,
            clocks.tx_interpolation,
        )
        .ok_or(Error::NoSampleRate)?;
        let rx_rate = rate(
            clocks.rx_converter_div,
            clocks.rx_tsp_div,
            clocks.rx_decimation,
        )
        .ok_or(Error::NoSampleRate)?;
        let (rx_lo, tx_lo) = self.control.lo();
        let int = |value: u32| i32::from_ne_bytes(value.to_ne_bytes());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "libusdr's `int deflogdur = rxsamplerate / 20e6`, non-negative"
        )]
        let log_duration = (f64::from(rx_rate) / 20e6) as u32;
        Ok(Plan {
            rx_rate: int(rx_rate),
            rx_lo: int(rx_lo),
            tx_lo: int(tx_lo),
            log_duration,
        })
    }

    /// One calibration's `find_best_2d`: its stages, from the current settings.
    fn search(&mut self, kind: Kind, channel: Mac, plan: &Plan) -> Result<[i32; 2], Error> {
        let golden = |limits| Stage {
            search: Search::Golden,
            limits,
            extra_step: 0,
        };
        let iterate = |limits, extra_step| Stage {
            search: Search::Iterate,
            limits,
            extra_step,
        };
        let iq = [IQ_PHASE_LIMITS, IQ_GAIN_LIMITS];
        let coarse = Limits {
            min: TX_LO_LIMITS.min / 8,
            max: TX_LO_LIMITS.max / 8,
        };
        let middle = Limits { min: -80, max: 80 };
        let stages: &[Stage] = match kind {
            Kind::RxLo => &[golden([RX_LO_LIMITS; 2])],
            Kind::TxLo => &[
                golden([TX_LO_LIMITS; 2]),
                golden([coarse; 2]),
                iterate([middle; 2], 4),
                iterate([FINE; 2], 0),
            ],
            Kind::RxIq | Kind::TxIq => &[golden(iq), golden(iq), iterate([FINE; 2], 0)],
        };
        let mut objective = Objective {
            device: self,
            kind,
            channel,
            log_duration: plan.log_duration,
        };
        let best = best_2d(stages, STOP_BELOW, &mut objective)?;
        Ok([best.x, best.y])
    }

    /// `calibrate_txlo`: the test signal, the receiver below the TX LO by an odd fraction
    /// of its rate with its filter and mixer on the leakage, then the search.
    fn calibrate_tx_lo(&mut self, channel: Mac, plan: &Plan) -> Result<[i32; 2], Error> {
        let (offset, level) = TX_LO_TONE;
        self.set_test_signal(channel, offset, level)?;
        let below = plan.offset(RX_TX_LO_DIVISOR);
        let rx = Direction::Rx;
        self.set_corr(channel, rx, Corr::Frequency(plan.tx_lo.wrapping_sub(below)))?;
        self.set_corr(channel, rx, Corr::Bandwidth(widened(below)))?;
        self.set_rx_nco_offset(channel, below.wrapping_neg())?;
        self.search(Kind::TxLo, channel, plan)
    }

    /// `calibrate_rxiqimb`: the TX LO set so the tone lands an odd fraction of the rate
    /// above the RX LO, then the I/Q search on its image.
    fn calibrate_rx_iq(&mut self, channel: Mac, plan: &Plan) -> Result<[i32; 2], Error> {
        let above = plan.offset(RX_IQ_DIVISOR);
        let tone = plan.rx_rate.wrapping_neg() / 7;
        let lo = plan.rx_lo.wrapping_add(above).wrapping_sub(tone);
        self.set_corr(channel, Direction::Tx, Corr::Frequency(lo))?;
        self.calibrate_iq(Kind::RxIq, channel, plan, tone, above, above.wrapping_neg())
    }

    /// `calibrate_txiqimb`: the RX LO below the TX LO by an odd fraction of the rate, the
    /// tone another fraction above the TX LO, then the I/Q search on its image.
    fn calibrate_tx_iq(&mut self, channel: Mac, plan: &Plan) -> Result<[i32; 2], Error> {
        let below = plan.offset(TX_IQ_DIVISOR);
        let tone = plan.offset(RX_IQ_DIVISOR);
        let lo = plan.tx_lo.wrapping_sub(below);
        self.set_corr(channel, Direction::Rx, Corr::Frequency(lo))?;
        let image = below.wrapping_neg().wrapping_add(tone);
        let real = below.wrapping_neg().wrapping_sub(tone);
        self.calibrate_iq(Kind::TxIq, channel, plan, tone, image, real)
    }

    /// `_calibrate_iqimb_generic`: the filters for the offsets, the tone's level found on
    /// the real component (`real`, the RX mixer's offset), the mixer moved to the image
    /// (`image`), then the search.
    fn calibrate_iq(
        &mut self,
        kind: Kind,
        channel: Mac,
        plan: &Plan,
        tone: i32,
        image: i32,
        real: i32,
    ) -> Result<[i32; 2], Error> {
        let required = real.unsigned_abs().max(image.unsigned_abs());
        let required = i32::try_from(required).unwrap_or(i32::MAX);
        self.set_corr(channel, Direction::Rx, Corr::Bandwidth(widened(required)))?;
        let tone_bandwidth = widened(i32::try_from(tone.unsigned_abs()).unwrap_or(i32::MAX));
        self.set_corr(channel, Direction::Tx, Corr::Bandwidth(tone_bandwidth))?;
        self.set_rx_nco_offset(channel, real)?;
        self.raise_tone(channel, plan, tone, kind == Kind::TxIq)?;
        self.set_rx_nco_offset(channel, image)?;
        self.measure(channel_index(channel), plan.log_duration)?;
        self.search(kind, channel, plan)
    }

    /// `_calibrate_txpwr`: the tone's level doubled from 127 until it measures above
    /// −7 dB; still below −10 dB, the TX amplifier's gain correction raised (the RX side
    /// has none: libusdr's `-E2BIG` at once for the TX calibration) until above −10 dB.
    fn raise_tone(
        &mut self,
        channel: Mac,
        plan: &Plan,
        tone: i32,
        tx_calibration: bool,
    ) -> Result<(), Error> {
        let index = channel_index(channel);
        let mut power = -120_000;
        let mut amplitude = 128_u32;
        while amplitude <= 32_768 {
            self.set_test_signal(channel, tone, amplitude - 1)?;
            power = self.measure(index, plan.log_duration)?;
            if power > LOUD_ENOUGH {
                break;
            }
            amplitude <<= 1;
        }
        if power < GAIN_ENOUGH {
            if tx_calibration {
                // `lms7002m_set_corr_param` selects the bank before refusing an RX gain.
                self.run_chip(&[ChipCall::MacSet(channel)])?;
            } else {
                for correction in 0..32_i8 {
                    self.run_chip(&[ChipCall::MacSet(channel), ChipCall::TbbGain(correction)])?;
                    power = self.measure(index, plan.log_duration)?;
                    if power > GAIN_ENOUGH {
                        break;
                    }
                }
            }
        }
        if power < TOO_WEAK {
            return Err(Error::WeakCalibrationSignal);
        }
        Ok(())
    }

    /// `lms7002m_set_corr_param` for `channel` in `direction`: its bank selected, then the
    /// setting.
    fn set_corr(&mut self, channel: Mac, direction: Direction, corr: Corr) -> Result<(), Error> {
        let index = channel_index(channel);
        let tsp = match direction {
            Direction::Rx => Tsp::Rx,
            Direction::Tx => Tsp::Tx,
        };
        let select = ChipCall::MacSet(channel);
        match corr {
            Corr::Dc(axis, value) => {
                let dacs = match direction {
                    Direction::Rx => [DcDac::RxAI, DcDac::RxAQ, DcDac::RxBI, DcDac::RxBQ],
                    Direction::Tx => [DcDac::TxAI, DcDac::TxAQ, DcDac::TxBI, DcDac::TxBQ],
                };
                let dac = dacs[2 * index + usize::from(axis == Axis::Y)];
                // `int16_t v`: the low 16 bits.
                let [low, high, ..] = value.to_le_bytes();
                let value = i16::from_le_bytes([low, high]);
                self.run_chip(&[select, ChipCall::DcDac(dac, value)])
            }
            Corr::Phase(value) => {
                self.run_chip(&[select, ChipCall::TspPhaseCorrection(tsp, value)])
            }
            Corr::GainBalance(value) => {
                let full = 2047_i32;
                let (i, q) = if value < 0 {
                    (full, full.wrapping_add(value))
                } else {
                    (full.wrapping_sub(value), full)
                };
                // The searches keep the balance within −1024 to 1023, so both codes are
                // 1023 to 2047.
                let code = |gain: i32| u16::try_from(gain).expect("invariant: 11-bit gain code");
                self.run_chip(&[select, ChipCall::TspGainCorrection(tsp, code(i), code(q))])
            }
            Corr::Frequency(value) => {
                self.run_chip(&[select])?;
                let synthesizer = match direction {
                    Direction::Rx => Synthesizer::Rx,
                    Direction::Tx => Synthesizer::Tx,
                };
                self.retune(synthesizer, u32::from_ne_bytes(value.to_ne_bytes()))?;
                self.run_chip(&[select])
            }
            Corr::Bandwidth(value) => {
                self.run_chip(&[select])?;
                let hertz = NonZeroU32::new(u32::from_ne_bytes(value.to_ne_bytes()))
                    .ok_or(Error::NoSampleRate)?;
                self.run_chip(&ctrl::bandwidth_calls(direction, hertz))
            }
        }
    }

    /// `lms7002m_sxx_tune(.., lo, false)` on the cached chip.
    fn retune(&mut self, synthesizer: Synthesizer, lo: u32) -> Result<(), Error> {
        let reference = self.control.reference();
        let mut tune = self
            .control
            .chip_mut()
            .sxx_tune(synthesizer, reference, lo, false);
        run(&mut *self.bus, &mut tune)?;
        match tune.outcome() {
            Some(Err(TuneError::OutOfRange)) => Err(Error::TuneOutOfRange(lo)),
            Some(Err(TuneError::ComparatorFailed)) => Err(Error::VcoComparatorFailed),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// `lms7002m_rbb_bandwidth` or `lms7002m_tbb_bandwidth` on the selected banks.
    fn set_filter(&mut self, direction: Direction, hertz: u32) -> Result<(), Error> {
        let hertz = NonZeroU32::new(hertz).ok_or(Error::NoSampleRate)?;
        self.run_chip(&ctrl::bandwidth_calls(direction, hertz))
    }

    /// `xsdrcal_set_nco_offset`: `channel`'s bank, then the RX mixer to `hertz`.
    fn set_rx_nco_offset(&mut self, channel: Mac, hertz: i32) -> Result<(), Error> {
        self.run_chip(&[ChipCall::MacSet(channel)])?;
        let word = self
            .control
            .nco_word(Direction::Rx, hertz)
            .map_err(|_| Error::NcoOutOfRange(hertz))?;
        self.run_chip(&[ChipCall::TspCmix(Tsp::Rx, word)])
    }

    /// `lms7002m_set_tx_testsig`: `channel`'s bank, then its TX processor's DC test signal
    /// at `level` (low 15 bits) shifted by `offset` hertz, or with [`NORMAL_INPUT`] its
    /// normal input again (the mixer left as it is).
    fn set_test_signal(&mut self, channel: Mac, offset: i32, level: u32) -> Result<(), Error> {
        let normal = level == NORMAL_INPUT;
        let generator = if normal {
            TspGenerator::Normal
        } else {
            TspGenerator::Dc
        };
        let [low, high, ..] = (level & 0x7fff).to_le_bytes();
        let level = i16::from_le_bytes([low, high]);
        self.run_chip(&[
            ChipCall::MacSet(channel),
            ChipCall::TspGenerator(Tsp::Tx, generator, level, level),
        ])?;
        if normal {
            return Ok(());
        }
        let word = self
            .control
            .nco_word(Direction::Tx, offset)
            .map_err(|_| Error::NcoOutOfRange(offset))?;
        self.run_chip(&[ChipCall::TspCmix(Tsp::Tx, word)])
    }

    /// `_xsdr_path_lb`: `channel`'s RF loopback into its LNA (`to_rx`) or from its TX band,
    /// then the loopback's attenuation to 0 dB.
    fn loop_back(
        &mut self,
        lna: RxPath,
        band: TxPath,
        channel: Mac,
        to_rx: bool,
    ) -> Result<(), Error> {
        let route = if to_rx {
            match lna {
                RxPath::LnaL => RxRoute::LoopbackL,
                RxPath::LnaW => RxRoute::LoopbackW,
                RxPath::LnaH => RxRoute::LoopbackH,
                RxPath::None => return Err(Error::InvalidPath),
            }
        } else {
            match band {
                TxPath::Band2 => RxRoute::LoopbackL,
                TxPath::Band1 => RxRoute::LoopbackW,
                TxPath::Mute => return Err(Error::InvalidPath),
            }
        };
        let op = self
            .control
            .set_rf_path(XsdrFrontend, channel, RfPath::Rx(route));
        self.run_control(op)?;
        self.run_chip(&[ChipCall::RfeGain(RxGainStage::Loopback, 0)])
    }

    /// The restore after the calibrations: the pad loss and mixers of each channel whose
    /// mute bit 0 (A) or 1 (B) was clear, as libusdr tests them; both paths, the mutes,
    /// `channel`'s bank, its TX gain correction if it moved, then both directions'
    /// baseband filters for the rates.
    fn restore(&mut self, channel: Mac, saved: &Saved) -> Result<(), Error> {
        let rx_ncos = self.control.nco_words(Direction::Rx);
        let tx_ncos = self.control.nco_words(Direction::Tx);
        for (index, bank) in [Mac::A, Mac::B].into_iter().enumerate() {
            if saved.mute & (1 << index) != 0 {
                continue;
            }
            self.run_chip(&[
                ChipCall::MacSet(bank),
                ChipCall::TrfGain(TxGainStage::Pad, -10 * i32::from(saved.tx_loss[index])),
                ChipCall::TspCmix(Tsp::Tx, tx_ncos[index].unwrap_or(0)),
                ChipCall::TspCmix(Tsp::Rx, rx_ncos[index].unwrap_or(0)),
            ])?;
        }
        self.run_chip(&[ChipCall::MacSet(Mac::Both)])?;
        let (rx_route, tx_route) = saved.routes;
        let op = self.control.set_rx_path(XsdrFrontend, rx_route);
        self.run_control(op)?;
        let op = self.control.set_tx_path(XsdrFrontend, tx_route);
        self.run_control(op)?;
        self.set_tx_mute(saved.mute)?;
        self.run_chip(&[ChipCall::MacSet(channel)])?;
        // libusdr saves the correction as `uint8_t` and compares it with the `int8_t`
        // cache, so a negative one never compares equal and is always rewritten: kept.
        let current = self
            .control
            .chip()
            .tbb_gain_correction(chip_channel(channel));
        if saved.gain_correction < 0 || current != saved.gain_correction {
            self.run_chip(&[ChipCall::TbbGain(saved.gain_correction)])?;
        }
        let (rx_rate, tx_rate) = super::quality::host_rates(self.rate);
        let tx = self.control.update_bandwidth(Direction::Tx, tx_rate);
        self.run_control(tx)?;
        let rx = self.control.update_bandwidth(Direction::Rx, rx_rate);
        self.run_control(rx)
    }

    /// `xsdrcal_do_meas_nco_avg`: channel `index`'s DC power, the best of
    /// `measurements` measurements over `2^?` (`log_duration + 1`, at most 255)
    /// accumulations, in millidB.
    fn measure(&mut self, index: usize, log_duration: u32) -> Result<i32, Error> {
        let accumulation = log_duration.saturating_add(1).min(255);
        let accumulation = u16::try_from(accumulation).unwrap_or(255);
        self.estimator_write(ESTIMATOR_ACCUMULATION, accumulation)?;
        let mut best = NO_MEASUREMENT;
        for _ in 0..self.measurements {
            self.estimator_write(ESTIMATOR_HOLD, 1)?;
            self.bus.sleep(Duration::from_micros(1));
            self.estimator_write(ESTIMATOR_HOLD, 0)?;
            let generation = self.estimator_read(ESTIMATOR_GENERATION)?;
            let mut measured = None;
            for _ in 0..ESTIMATOR_POLLS {
                if let Some(power) = self.dc_power(accumulation, generation, index)? {
                    measured = Some(power);
                    break;
                }
                self.bus.sleep(ESTIMATOR_POLL);
            }
            best = best.max(measured.ok_or(Error::EstimatorStalled)?);
        }
        Ok(best)
    }

    /// `xsdr_rfe_pwrdc_get`: once the generation has moved past `previous`, channel
    /// `index`'s I and Q read until the generation holds across them, as power in
    /// millidB; `None` while it has not moved (libusdr's `-EAGAIN`).
    fn dc_power(
        &mut self,
        accumulation: u16,
        previous: u32,
        index: usize,
    ) -> Result<Option<i32>, Error> {
        let first = ESTIMATOR_A_I + if index == 0 { 0 } else { 2 };
        let (i, q) = loop {
            let generation = self.estimator_read(ESTIMATOR_GENERATION)?;
            if generation == previous {
                return Ok(None);
            }
            let i = self.estimator_read(first)?;
            let q = self.estimator_read(first + 1)?;
            if self.estimator_read(ESTIMATOR_GENERATION)? == generation {
                break (i, q);
            }
        };
        let level = |word: u32| {
            let sum = f64::from(i32::from_ne_bytes(word.to_ne_bytes()));
            (f64::from(HALF_LSB) + sum / f64::from(accumulation) / 65_536.0) / 2048.0
        };
        let (i, q) = (level(i), level(q));
        let power = (i * i + q * q).max(1e-18);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr's double stored to an int, truncated; within ±180000"
        )]
        Ok(Some((1000.0 * 10.0 * power.log10()) as i32))
    }

    /// `xsdr_phy_dc_estim_start` or `_accum`: a write to the estimator's `address`.
    fn estimator_write(&mut self, address: u8, value: u16) -> Result<(), Error> {
        let access = RxPhyBank::DcEstimator.access(true, address, value);
        run(&mut *self.bus, &mut Step::fpga(access))
    }

    /// `xsdr_phy_dc_estim_get`: the estimator's word `address` selected, then read.
    fn estimator_read(&mut self, address: u8) -> Result<u32, Error> {
        let select = RxPhyBank::DcEstimator.access(false, address, 0);
        run(&mut *self.bus, &mut Step::fpga(select))?;
        let mut read = Step::fpga(FpgaAccess::Read { reg: REG_CFG_PHY_0 });
        run(&mut *self.bus, &mut read)?;
        Ok(read.value().unwrap_or_default())
    }
}

/// A search's evaluator (`_evaluate_rxtxlo` and its wrappers) over the device.
struct Objective<'a> {
    /// The device.
    device: &'a mut Device,
    /// The search.
    kind: Kind,
    /// The channel.
    channel: Mac,
    /// The measurements' log duration (`deflogdur`).
    log_duration: u32,
}

impl Objective<'_> {
    /// The setting `axis` at `value` moves.
    fn corr(&self, axis: Axis, value: i32) -> (Direction, Corr) {
        let direction = match self.kind {
            Kind::RxLo | Kind::RxIq => Direction::Rx,
            Kind::TxLo | Kind::TxIq => Direction::Tx,
        };
        let corr = match (self.kind, axis) {
            (Kind::RxLo | Kind::TxLo, _) => Corr::Dc(axis, value),
            (Kind::RxIq | Kind::TxIq, Axis::X) => Corr::Phase(value),
            (Kind::RxIq | Kind::TxIq, Axis::Y) => Corr::GainBalance(value),
        };
        (direction, corr)
    }
}

impl Objective2d for Objective<'_> {
    type Error = Error;

    fn set(&mut self, _stage: usize, axis: Axis, value: i32) -> Result<(), Error> {
        let (direction, corr) = self.corr(axis, value);
        // `find_best_2d` ignores these calls' results and searches on: kept.
        let _ = self.device.set_corr(self.channel, direction, corr);
        Ok(())
    }

    fn evaluate(&mut self, stage: usize, axis: Axis, value: i32) -> Result<i32, Error> {
        let (direction, corr) = self.corr(axis, value);
        self.device.set_corr(self.channel, direction, corr)?;
        // The TX LO search's last stage measures four times as long (`mulf`).
        let factor = if self.kind == Kind::TxLo && stage == 3 {
            4
        } else {
            1
        };
        let log_duration = self.log_duration.wrapping_mul(factor);
        self.device
            .measure(channel_index(self.channel), log_duration)
    }
}

/// The chip's channel `channel` names (A for anything but B).
fn chip_channel(channel: Mac) -> Channel {
    if channel == Mac::B {
        Channel::B
    } else {
        Channel::A
    }
}

/// 0 for channel A, 1 for B.
fn channel_index(channel: Mac) -> usize {
    usize::from(channel == Mac::B)
}

/// A filter bandwidth for an offset of `hertz`: `ABS(offset) * 2.33f`, in single
/// precision, truncated back to an `int` as libusdr's argument conversion does.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "libusdr's int-times-float product stored to an int"
)]
fn widened(hertz: i32) -> i32 {
    (hertz.unsigned_abs() as f32 * BANDWIDTH_FACTOR) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offsets_follow_libusdrs_fractions() {
        let plan = Plan {
            rx_rate: 40_000_000,
            rx_lo: 0,
            tx_lo: 0,
            log_duration: 2,
        };
        // libusdr's logs: 1737717 Hz for the TX LO search, 7838372 for the RX I/Q one.
        assert_eq!(plan.offset(RX_TX_LO_DIVISOR), 1_737_717);
        assert_eq!(plan.offset(RX_IQ_DIVISOR), 7_838_372);
        assert_eq!(widened(-1_737_717), 4_048_880);
    }
}
