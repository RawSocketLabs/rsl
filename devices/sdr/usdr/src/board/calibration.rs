//! LO-leakage and IQ-imbalance calibration: `usdr_calibrate` in
//! `device/m2_lm6_1/usdr_ctrl.c`, as `/dm/sdr/0/calibrate` reaches it
//! (`usdr_device_m2_lm6_1_calibrate_set`), over the searches of `cal/cal_lo_iqimb.c`.
//!
//! Each calibration loops the transmitter into the receiver (the LMS6002D's RF loopback,
//! or a cable), sends a test tone from the TX chain's constant generator and NCOs, tunes
//! the other chain so the component it minimises lands on the RX NCO, and searches the
//! correction with [`best_2d`] against the FPGA's DC estimator (`fpga::dc_estimator`):
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
//! Everything here is register access, so it runs as blocking code like the other
//! sequences; it does not stream.

use std::time::Duration;

use bnb::{u5, u12};
use deps::tracing::debug;
use num_complex::Complex;

use super::Board;
use super::correction::TxDcOffset;
use crate::cal::search::{Axis, Best2d, Limits, Objective2d, Search, Stage, best_2d};
use crate::chips::lms6002d::Lna;
use crate::chips::register::run_sequence;
use crate::error::Error;
use crate::fpga::{IqImbalance, Nco, Phy, dc_estimator};
use crate::portable::radio::state::nco_word;
use crate::portable::radio::{Chain, RxGain};

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

/// The correction an [`Objective2d`] moves, X then Y (`usdr_set_corr_param`).
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

/// A search over `target` on the board.
struct Objective<'a> {
    /// The board.
    board: &'a mut Board,

    /// What the search moves.
    target: Target,
}

impl Objective2d for Objective<'_> {
    type Error = Error;

    fn set(&mut self, _stage: usize, axis: Axis, value: i32) -> Result<(), Error> {
        self.board.set_correction(self.target, axis, value)
    }

    fn evaluate(&mut self, stage: usize, axis: Axis, value: i32) -> Result<i32, Error> {
        self.set(stage, axis, value)?;
        dc_estimator::measure(self.board.bus.as_mut(), CHANNEL)
    }
}

impl Board {
    /// Runs the calibrations `request` asks for, in libusdr's order (RX IQ, then TX LO, then
    /// TX IQ), and restores the board's tuning and filters (`usdr_calibrate`).
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
        let decimation = self.radio.rx.decimation.ok_or(Error::NoSampleRate)?;
        let external = request.feedback == Feedback::External;
        let lowered = |lo: u32, offset: i32| {
            if lo == 0 {
                0
            } else {
                lo.wrapping_sub(u32::from_ne_bytes(offset.to_ne_bytes()))
            }
        };
        let tx_lo = lowered(self.radio.tx.lo_hz, self.radio.tx.lo_offset_hz);
        let rx_lo = lowered(self.radio.rx.pll_lo_hz, self.radio.rx.lo_offset_hz);
        let rx_input = self.radio.lms.active_lna();
        let rate = self.radio.adc_rate_hz / decimation.factor();
        let as_int = |hz: u32| i32::from_ne_bytes(hz.to_ne_bytes());
        let mut plan = Plan {
            rx_rate: as_int(rate),
            rx_hz: as_int(rx_lo),
            tx_hz: as_int(tx_lo),
        };
        if self.radio.tx.lo_offset_hz != 0 {
            // Extend the measurement to fit RX: libusdr's `int += double`, with its TX
            // rate (`dac_clk / txbb_intr`), which equals the RX rate on the uSDR.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "libusdr converts the double sum back to int"
            )]
            let extended = (f64::from(plan.tx_hz) + 0.2 * f64::from(plan.rx_rate)) as i32;
            plan.tx_hz = extended;
        }
        let mut report = CalibrationReport::default();

        if request.rx_iq_imbalance && rx_lo > 0 {
            if tx_lo == 0 || !self.radio.tx.powered {
                self.set_tx_power(true)?;
                self.bus.sleep(WAKE);
                Phy::Tx.set_iq_correction(self.bus.as_mut(), self.corrections.tx.tx_terms())?;
            }
            if !external {
                run_sequence(self.bus.as_mut(), &mut self.radio.lms.enable_rf_loopback())?;
            }
            report.rx_iq_imbalance = Some(self.calibrate_rx_iq(&plan)?);
            let mut restored = if tx_lo == 0 {
                run_sequence(self.bus.as_mut(), &mut self.radio.lms.disable_tx_pll())
            } else {
                match self.tune_tx_pll(tx_lo) {
                    Err(Error::UnsupportedFrequency(_)) => {
                        run_sequence(self.bus.as_mut(), &mut self.radio.lms.disable_tx_pll())
                    }
                    Err(Error::PllUnlocked(_)) | Ok(_) => Ok(()),
                    Err(error) => Err(error),
                }
            };
            if !external && restored.is_ok() {
                restored =
                    run_sequence(self.bus.as_mut(), &mut self.radio.lms.disable_rf_loopback());
            }
            restored?;
        }

        if (request.tx_lo_leakage || request.tx_iq_imbalance) && tx_lo > 0 {
            self.prepare_tx_calibration(&plan, rx_lo, external)?;
            if request.tx_lo_leakage {
                report.tx_lo_leakage = Some(self.calibrate_tx_lo(&plan, request.analog_lo_first)?);
            }
            if request.tx_iq_imbalance {
                let vga1 = self.radio.lms.tx_vga1_code();
                run_sequence(
                    self.bus.as_mut(),
                    &mut self.radio.lms.set_tx_vga1_code(u5::new(TX_IQ_VGA1_CODE)),
                )?;
                report.tx_iq_imbalance = Some(self.calibrate_tx_iq(&plan)?);
                run_sequence(
                    self.bus.as_mut(),
                    &mut self.radio.lms.set_tx_vga1_code(vga1),
                )?;
            }
            self.finish_tx_calibration(tx_lo, rx_lo, rx_input, external)?;
        }

        let restored = self.restore_after_calibration();
        // libusdr powers down what was not running even after a failed restore, and
        // ignores how that goes.
        if rx_lo == 0 {
            let _ = self.set_rx_power(false);
        }
        if tx_lo == 0 {
            let _ = self.set_tx_power(false);
        }
        restored?;
        debug!(?report, "calibrated");
        Ok(report)
    }

    /// Readies the receiver for the TX calibrations: powers and DC-calibrates it if it was
    /// down or untuned, retunes the TX PLL, and loops the transmitter into LNA1.
    fn prepare_tx_calibration(
        &mut self,
        plan: &Plan,
        rx_lo: u32,
        external: bool,
    ) -> Result<(), Error> {
        let mut retune_tx = true;
        if rx_lo == 0 || !self.radio.rx.powered {
            self.set_rx_power(true)?;
            self.bus.sleep(WAKE);
            self.tune_rx_pll(RX_WAKE_LO_HZ)?;
            self.calibrate_rx_dc()?;
            Phy::Rx.set_iq_correction(self.bus.as_mut(), self.corrections.rx.rx_terms())?;
            retune_tx = false;
        }
        if retune_tx || self.radio.tx.lo_offset_hz != 0 {
            self.tune_tx_pll(u32::from_ne_bytes(plan.tx_hz.to_ne_bytes()))?;
        }
        if !external {
            run_sequence(self.bus.as_mut(), &mut self.radio.lms.select_lna(Lna::Lna1))?;
            run_sequence(self.bus.as_mut(), &mut self.radio.lms.enable_rf_loopback())?;
        }
        Ok(())
    }

    /// Restores what the TX calibrations moved: the TX PLL if it ran on an NCO offset,
    /// the RX PLL (or turns it off if it had no LO), the RX input and loopback, and
    /// RXVGA1's gain. As in libusdr, a failed step skips the rest, except that a PLL out
    /// of range (the TX PLL's retune, or else the RX PLL's) turns the RX PLL off, and one
    /// that will not lock is not an error: libusdr checks the RX step's status, which
    /// still holds a failed TX retune's.
    fn finish_tx_calibration(
        &mut self,
        tx_lo: u32,
        rx_lo: u32,
        rx_input: Lna,
        external: bool,
    ) -> Result<(), Error> {
        let mut restored = Ok(());
        if self.radio.tx.lo_offset_hz != 0 {
            restored = self.tune_tx_pll(tx_lo).map(drop);
        }
        if rx_lo > 0 {
            if restored.is_ok() {
                restored = self.tune_rx_pll(rx_lo).map(drop);
            }
            restored = match restored {
                Err(Error::UnsupportedFrequency(_)) => {
                    run_sequence(self.bus.as_mut(), &mut self.radio.lms.disable_rx_pll())
                }
                Err(Error::PllUnlocked(_)) => Ok(()),
                other => other,
            };
        } else if restored.is_ok() {
            restored = run_sequence(self.bus.as_mut(), &mut self.radio.lms.disable_rx_pll());
        }
        restored?;
        let bus = self.bus.as_mut();
        if !external {
            run_sequence(bus, &mut self.radio.lms.select_lna(rx_input))?;
            run_sequence(bus, &mut self.radio.lms.disable_rf_loopback())?;
        }
        run_sequence(
            bus,
            &mut self
                .radio
                .lms
                .set_rx_vga1_gain(i32::from(self.radio.rx.vga1_db) + RxGain::VGA1_MIN_DB),
        )
    }

    /// Turns the constant generator off and points the NCOs and filters back where the
    /// caller's settings put them.
    fn restore_after_calibration(&mut self) -> Result<(), Error> {
        self.set_tx_constant(None)?;
        self.restore_ncos(Chain::Tx)?;
        self.restore_rx_ncos()?;
        match self.radio.rx.fixed_bandwidth_hz {
            Some(hz) => run_sequence(self.bus.as_mut(), &mut self.radio.lms.set_rx_bandwidth(hz))?,
            None => self.update_rx_bandwidth()?,
        }
        match self.radio.tx.fixed_bandwidth_hz {
            Some(hz) => run_sequence(self.bus.as_mut(), &mut self.radio.lms.set_tx_bandwidth(hz)),
            None => self.update_tx_bandwidth(),
        }
    }

    /// The RX IQ search (`calibrate_rxiqimb`): the TX LO a fifth of the RX rate above the
    /// RX LO, the tone on it, its image nulled.
    fn calibrate_rx_iq(&mut self, plan: &Plan) -> Result<IqCalibration, Error> {
        let offset = plan.offset(RX_IQ_FRACTION);
        self.tune_for_calibration(Chain::Tx, plan.rx_hz.wrapping_add(offset))?;
        self.calibrate_iq(Target::RxIq, 0, offset, -offset)
    }

    /// The TX IQ search (`calibrate_txiqimb`): the RX LO a ninth of the RX rate below the
    /// TX LO, the tone a fifth above it, its image nulled.
    fn calibrate_tx_iq(&mut self, plan: &Plan) -> Result<IqCalibration, Error> {
        let below = plan.offset(TX_IQ_FRACTION);
        let tone = plan.offset(RX_IQ_FRACTION);
        self.tune_for_calibration(Chain::Rx, plan.tx_hz.wrapping_sub(below))?;
        self.calibrate_iq(Target::TxIq, tone, tone - below, -below - tone)
    }

    /// The IQ searches' shared part (`_calibrate_iqimb_generic`): open the filters, level
    /// the tone at `tone_hz` with the RX NCO on it (`signal_hz`), then search with the NCO
    /// on its image (`image_hz`).
    fn calibrate_iq(
        &mut self,
        target: Target,
        tone_hz: i32,
        image_hz: i32,
        signal_hz: i32,
    ) -> Result<IqCalibration, Error> {
        let needed = signal_hz.abs().max(image_hz.abs());
        self.set_calibration_bandwidth(Chain::Rx, needed)?;
        self.set_calibration_bandwidth(Chain::Tx, tone_hz.abs())?;
        self.set_rx_nco_offset(signal_hz)?;
        // The other chain's gain makes up a weak loop.
        let boost = if target == Target::TxIq {
            Chain::Rx
        } else {
            Chain::Tx
        };
        let Some(signal) = self.level_tone(tone_hz, boost)? else {
            return Ok(IqCalibration::SignalTooLow);
        };
        self.set_rx_nco_offset(image_hz)?;
        let image = dc_estimator::measure(self.bus.as_mut(), CHANNEL)?;

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
        let stages = [
            Stage {
                search: Search::Golden,
                limits: full,
                extra_step: 0,
            },
            Stage {
                search: Search::Golden,
                limits: full,
                extra_step: 0,
            },
            Stage {
                search: Search::Iterate,
                limits: [fine, fine],
                extra_step: 0,
            },
        ];
        let best = self.search(&stages, target)?;
        debug!(?target, signal, image, ?best, "IQ imbalance calibrated");
        Ok(IqCalibration::Corrected(IqImbalance {
            amplitude: best.y,
            phase: best.x,
        }))
    }

    /// The LO-leakage search (`calibrate_txlo`), first over TXVGA1's DC shift if
    /// `analog_first`, then over the FPGA's DC offset.
    fn calibrate_tx_lo(&mut self, plan: &Plan, analog_first: bool) -> Result<TxLoLeakage, Error> {
        // libusdr narrows the limits for the analog search and never widens them again.
        let limit = if analog_first {
            Limits {
                min: -128,
                max: 127,
            }
        } else {
            Limits {
                min: -i32::from(TxDcOffset::MAX),
                max: i32::from(TxDcOffset::MAX),
            }
        };
        if analog_first {
            self.search_tx_lo(plan, limit, false)?;
        }
        let best = self.search_tx_lo(plan, limit, true)?;
        let offset =
            |value| TxDcOffset::new(value).expect("invariant: searched within the 12-bit range");
        let leakage = TxLoLeakage {
            i: offset(self.corrections.tx_lo_dc[0]),
            q: offset(self.corrections.tx_lo_dc[1]),
        };
        debug!(?best, ?leakage, "TX LO leakage calibrated");
        Ok(leakage)
    }

    /// One LO-leakage search within `limit`.
    fn search_tx_lo(&mut self, plan: &Plan, limit: Limits, digital: bool) -> Result<Best2d, Error> {
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
            // libusdr measures four times longer here, but its estimator ignores the
            // duration.
            stage(Search::Iterate, Limits { min: -8, max: 8 }, 0),
        ];
        self.set_test_tone(TX_LO_TONE_HZ, TX_LO_TONE_POWER)?;
        let offset = plan.offset(TX_LO_FRACTION);
        self.tune_for_calibration(Chain::Rx, plan.tx_hz.wrapping_sub(offset))?;
        self.set_calibration_bandwidth(Chain::Rx, offset.abs())?;
        self.set_rx_nco_offset(-offset)?;
        self.search(&stages, Target::TxLo { digital })
    }

    /// Runs [`best_2d`] over `stages`, moving `target`. libusdr's coarse modes, which would
    /// leave out the last stages, never apply: `usdrcal_init_calibrate` zeroes the mode
    /// after `usdr_calibrate` sets it.
    fn search(&mut self, stages: &[Stage], target: Target) -> Result<Best2d, Error> {
        let mut objective = Objective {
            board: self,
            target,
        };
        best_2d(stages, STOP_BELOW, &mut objective)
    }

    /// Raises the tone at `tone_hz` until it measures above -7 dBFS, then, if it is still
    /// under -10 dBFS, `boost`'s VGA1 gain (the chain not being calibrated) until it is
    /// above or at the stage's maximum (`_calibrate_txpwr`). Returns the level, or `None`
    /// under -70 dBFS.
    fn level_tone(&mut self, tone_hz: i32, boost: Chain) -> Result<Option<i32>, Error> {
        let mut level = -120_000;
        let mut amplitude = 128_u32;
        while amplitude <= 32_768 {
            self.set_test_tone(tone_hz, amplitude - 1)?;
            level = dc_estimator::measure(self.bus.as_mut(), CHANNEL)?;
            if level > -7000 {
                break;
            }
            amplitude <<= 1;
        }
        if level < -10_000 {
            for step in 0..32 {
                if !self.raise_gain(boost, step)? {
                    break;
                }
                level = dc_estimator::measure(self.bus.as_mut(), CHANNEL)?;
                if level > -10_000 {
                    break;
                }
            }
        }
        debug!(level, "calibration tone levelled");
        Ok((level >= -70_000).then_some(level))
    }

    /// Sets RXVGA1 `step` dB above the caller's gain, or TXVGA1 to code `10 + step`
    /// (`CORR_OP_SET_GAIN`). Past the stage's maximum (libusdr's `-E2BIG`) it writes
    /// nothing and returns `false`.
    fn raise_gain(&mut self, chain: Chain, step: u8) -> Result<bool, Error> {
        let bus = self.bus.as_mut();
        match chain {
            Chain::Rx => {
                let db = i32::from(self.radio.rx.vga1_db) + i32::from(step) + RxGain::VGA1_MIN_DB;
                if db > RxGain::VGA1_MAX_DB {
                    return Ok(false);
                }
                run_sequence(bus, &mut self.radio.lms.set_rx_vga1_gain(db))?;
            }
            Chain::Tx => {
                if step > 21 {
                    return Ok(false);
                }
                run_sequence(
                    bus,
                    &mut self.radio.lms.set_tx_vga1_code(u5::new(10 + step)),
                )?;
            }
        }
        Ok(true)
    }

    /// Sends the constant generator's I at `power` (at most 30000), Q 0, shifted by both TX
    /// NCOs to `offset_hz` (`usdr_tx_gen_set`, `_usdr_set_nco`).
    fn set_test_tone(&mut self, offset_hz: i32, power: u32) -> Result<(), Error> {
        let level = i16::try_from(power.min(TONE_MAX)).expect("invariant: at most 30000");
        self.set_tx_constant((power != 0).then_some(Complex::new(level, 0)))?;
        let word = nco_word(offset_hz, self.radio.adc_rate_hz);
        let bus = self.bus.as_mut();
        Phy::Tx.set_nco(bus, Nco::Nco0, word)?;
        Phy::Tx.set_nco(bus, Nco::Nco1, word)
    }

    /// Points the RX NCO of the estimator's channel at `offset_hz`.
    fn set_rx_nco_offset(&mut self, offset_hz: i32) -> Result<(), Error> {
        let word = nco_word(offset_hz, self.radio.adc_rate_hz);
        Phy::Rx.set_nco(self.bus.as_mut(), Nco::Nco0, word)
    }

    /// Tunes `chain`'s PLL to `hz`, a C `int` as libusdr passes it (`CORR_OP_SET_FREQ`).
    fn tune_for_calibration(&mut self, chain: Chain, hz: i32) -> Result<(), Error> {
        self.tune_pll(chain, u32::from_ne_bytes(hz.to_ne_bytes()))
            .map(drop)
    }

    /// Opens `chain`'s filter to [`BANDWIDTH_FACTOR`] times `hz`, in libusdr's float
    /// arithmetic (`CORR_OP_SET_BW`).
    fn set_calibration_bandwidth(&mut self, chain: Chain, hz: i32) -> Result<(), Error> {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            reason = "libusdr multiplies the int by a float and passes the product as an int"
        )]
        let bandwidth = (hz as f32 * BANDWIDTH_FACTOR) as i32 as u32;
        let bus = self.bus.as_mut();
        match chain {
            Chain::Rx => run_sequence(bus, &mut self.radio.lms.set_rx_bandwidth(bandwidth)),
            Chain::Tx => run_sequence(bus, &mut self.radio.lms.set_tx_bandwidth(bandwidth)),
        }
    }

    /// Moves `target`'s `axis` to `value` (`usdr_set_corr_param`).
    fn set_correction(&mut self, target: Target, axis: Axis, value: i32) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        match (target, axis) {
            (Target::TxLo { digital }, axis) => {
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
                    Phy::set_tx_dc_offset(bus, bits(i), bits(q))
                } else {
                    // `usdr_tx_vga1_dccorr`: narrowed to `int8_t`, then offset binary.
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "libusdr narrows to int8_t and adds 0x80 in eight bits"
                    )]
                    let code = |v: i16| (v as i8 as u8).wrapping_add(0x80);
                    run_sequence(bus, &mut self.radio.lms.set_tx_vga1_dc(code(i), code(q)))
                }
            }
            (Target::RxIq, axis) => {
                self.corrections.rx.set(axis, value);
                Phy::Rx.set_iq_correction(bus, self.corrections.rx.rx_terms())
            }
            (Target::TxIq, axis) => {
                self.corrections.tx.set(axis, value);
                Phy::Tx.set_iq_correction(bus, self.corrections.tx.tx_terms())
            }
        }
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
