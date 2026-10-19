//! Setting an XSDR's sample rate: libusdr's `xsdr_set_samplerate_ex`
//! (`device/m2_lm7_1/xsdr_ctrl.c`) on gateware with a TX MMCM and no RX MMCM or DUC/DDC.
//!
//! The AFE comes on (first time only), the LMS7002M's rate is set
//! (`lms7002m_samplerate`, in `lms7002m`'s control code), the `LimeLight` receive
//! link is calibrated (`_xsdr_calibrate_lml`: RX streamed up, the deserializer reset and
//! the clock phase swept against the LFSR checkers until a band of error-free phases is
//! found), bank A selected, and the processors' self-tests waited out.
//!
//! Without a TX MMCM libusdr's calibration writes a fixed TX delay and then fails the
//! rate with `-ERANGE`, its RX badness never having been measured; that is reproduced.

use hal::{
    FpgaAccess, I2cAddr, IndexedRegister, RegisterAccess, Sequence, SequenceAction, SequenceFailure,
};
use lms7002m::Batch;
use lms7002m::ctrl::{Control, ControlError, Operation, RateFlags, RateRequest, StreamFlags};
use lms7002m::{BistCheck, Mac, Tsp};
use lp8758::{self, Buck, BuckVoltage};

use super::fpga::{
    Gpo, LMS_LDO_ENABLE, LMS_RESET_RELEASE, LMS_RX_ENABLE, LMS_TX_ENABLE, REG_CFG_PHY_0, RxPhyBank,
    TX_DEFAULT_DELAY,
};
use super::frontend::XsdrFrontend;
use super::step::Step;

/// Above this rate libusdr switches to double-pumped SISO DDR (`XSDR_SISO_RATE`).
const SISO_RATE: u32 = 72_000_000;
/// Above this rate libusdr raises the I/O and core supplies for the link.
const BOOST_RATE: u32 = 70_000_000;
/// The I/O supply's boost: channel 1 to 1.94 V (`xsdr_set_vio(d, 1940)`, code 0xB8).
const VIO_BOOST: (Buck, BuckVoltage) = (Buck::B1, BuckVoltage::from_millivolts(1940));
/// The LMS7002M core supply's boost: channel 2, which libusdr asks at 1.32 V
/// (`xsdr_set_lms125vdd(d, 1320)`) and writes code 0x8E, 1.325 V (SNVSAC6B; libusdr's
/// `s_get_vout` sits one code high there, as for the 1.28 V rail at open).
const VDD_BOOST: (Buck, BuckVoltage) = (Buck::B2, BuckVoltage::from_millivolts(1325));
/// Phase-calibration retries (`MAX_RTY`).
const RETRIES: u8 = 5;
/// The phases swept: 1 to 4 * 63 (`ph`).
const PHASES: u16 = 4 * 63 + 1;
/// Checker reads per phase (`check_to`).
const CHECKS: u8 = 10;
/// The deserializer's reset word, bits 2:0 (`0x80000007`, truncated to the 16-bit value).
const DESER_RESET: u16 = 0x0007;
/// The badness above which a failed calibration fails the rate.
const SEVERE: u64 = 100;
/// The LFSR counters' read selector: `LFSR_CNTR_BER` in bits 3:2.
const BER: u8 = 2 << 2;

/// The gateware features the rate path reads from HWID's PHY bits (`xsdr_init`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gateware {
    /// A TX MMCM (`PHY_CFG_TX_MMCM`).
    pub(crate) tx_mmcm: bool,

    /// RX samples on `LimeLight` port 1 (`PHY_CFG_LML2_IS_RX` clear).
    pub(crate) rx_port1: bool,
}

/// The board's sample-rate state that outlives one call (`xsdr_dev_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RateState {
    /// The AFE has been enabled for reference clocking (`afe_active`).
    pub(crate) afe_active: bool,

    /// Double-pumped SISO DDR (`dpump`), chosen at the first rate above 72 MS/s.
    pub(crate) double_pump: bool,

    /// The RX rate last set (`s_rxrate`).
    pub(crate) rx: u32,

    /// The TX rate last set (`s_txrate`).
    pub(crate) tx: u32,
}

/// Why a rate was not set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RateError {
    /// The control code refused it.
    Control(ControlError),

    /// The `LimeLight` calibration found no clean phase, or (without a TX MMCM) measured
    /// none: libusdr's `-ERANGE` ("LML Calibration failed").
    Calibration,
}

/// Where a phase probe is: one setting of the RX link tested against the LFSR checkers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Probe {
    /// The 10 µs before a swept phase.
    Lead,
    /// The FIFO reset pulse.
    FifoReset,
    /// The deserializer held in reset.
    DeserReset,
    /// 10 µs.
    DeserWait,
    /// The deserializer released.
    DeserRun,
    /// 10 µs.
    Settle,
    /// The checkers' enable writes, 0 to 2.
    Checker(u8),
    /// Check `check`: its 100 µs.
    CheckWait(u8),
    /// Check `check`, counter `counter`: the selection write.
    Select(u8, u8),
    /// Its 1 µs.
    SelectWait(u8, u8),
    /// Its read.
    Read(u8, u8),
}

/// The phase sweep's state (`_xsdr_calibrate_lml`'s RX autocalibration).
#[derive(Clone, Copy, Debug)]
struct Sweep {
    /// The retry, 0 to 4.
    retry: u8,

    /// The phase probed, or 0 for the best-effort probe after a sweep.
    phase: u16,

    /// The lowest and highest clean phases (`phase_min`, `phase_max`).
    clean: (i32, i32),

    /// The phase kept (`phase_m`).
    kept: i32,

    /// The last phase probed (`phase_last`).
    last: i32,

    /// The least badness seen (`badness_m`).
    least: u64,

    /// The last probe was clean (`last_noerrs`).
    last_clean: bool,

    /// The checkers' counters as last read (`errs`).
    errors: [u32; 4],

    /// Where the probe is.
    probe: Probe,

    /// The badness the last check computed.
    badness: u64,
}

/// Where the rate setting is.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// The AFE on for reference clocking (`afe_active` was false); failures ignored.
    Afe,
    /// 10 ms for the clocks.
    AfeSettle,
    /// The LMS7002M's rate.
    Rate,
    /// The I/O supply boost.
    VioBoost,
    /// The core supply boost.
    VddBoost,
    /// Without a TX MMCM: the fixed delay.
    TxDelay,
    /// The LMS7002M's RX and TX pins on, before streaming RX up.
    RxPower,
    /// 1 ms.
    RxPowerSettle,
    /// RX streamed up on both channels.
    StreamUp,
    /// The `LimeLight` reset pulse.
    LimelightReset,
    /// The deserializer reset: 0 hold, 1 wait, 2 release.
    Deser(u8),
    /// A retry's switch of the RX FIFO to the LFSR pattern.
    LfsrMode,
    /// A phase probe.
    Probe,
    /// The RX FIFO back to the processor (RX was running before).
    NormalMode,
    /// Bank A selected.
    SelectA,
    /// 25 ms before each self-test check.
    BistWait,
    /// The RX processor's self-test status.
    BistRx,
    /// The TX processor's.
    BistTx,
    /// Finished.
    Done(Result<(), RateError>),
}

/// The part running.
#[derive(Clone, Copy, Debug)]
enum Part {
    /// Chip writes.
    Chip(Batch),
    /// A control-code operation without board hooks.
    Control(Operation),
    /// RX streaming up, which switches the antenna.
    StreamUp(Operation<XsdrFrontend>),
    /// One board action.
    Step(Step),
    /// A self-test check.
    Bist(BistCheck),
}

impl Part {
    /// The next action.
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Chip(batch) => batch.next(),
            Self::Control(op) => op.next(),
            Self::StreamUp(op) => op.next(),
            Self::Step(step) => step.next(),
            Self::Bist(check) => check.next(),
        }
    }

    /// Applies an action's outcome.
    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        match self {
            Self::Chip(batch) => batch.complete(result),
            Self::Control(op) => op.complete(result),
            Self::StreamUp(op) => op.complete(result),
            Self::Step(step) => step.complete(result),
            Self::Bist(check) => check.complete(result),
        }
    }
}

/// `xsdr_set_samplerate_ex` with libusdr's `/dm/rate` flags (maximum converter rate,
/// extended CGEN), on a powered XSDR.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SetSampleRate {
    /// The LMS7002M control state.
    control: Control,

    /// The board's rate state.
    state: RateState,

    /// The gateware.
    gateware: Gateware,

    /// The PMIC the supply boost writes.
    pmic: I2cAddr,

    /// The control request.
    request: RateRequest,

    /// RX was running before (`old_rx_run`).
    was_running: bool,

    /// The RX calibration's measured badness (`rx_badness`).
    rx_badness: u64,

    /// The phase sweep.
    sweep: Sweep,

    /// The RX processor's self-test had finished at the last check.
    bist_rx_done: bool,

    /// Where the setting is.
    stage: Stage,

    /// The part running.
    part: Part,

    /// A bus access failed.
    failed: bool,
}

impl SetSampleRate {
    /// Starts setting `rx` and `tx` samples per second (either may be 0, not both).
    pub(crate) fn new(
        control: &Control,
        state: RateState,
        gateware: Gateware,
        pmic: I2cAddr,
        (rx, tx): (u32, u32),
    ) -> Self {
        let mut rate = Self {
            control: *control,
            state,
            gateware,
            pmic,
            request: RateRequest {
                rx,
                tx,
                adc: 0,
                dac: 0,
                flags: RateFlags::default(),
                rx_port1: gateware.rx_port1,
                rx_decimation: 1,
                tx_interpolation: 1,
            },
            was_running: false,
            rx_badness: u64::MAX,
            sweep: Sweep {
                retry: 0,
                phase: 1,
                clean: (i32::from(PHASES), 0),
                kept: 0,
                last: -1,
                least: u64::MAX,
                last_clean: false,
                errors: [u32::MAX; 4],
                probe: Probe::Lead,
                badness: u64::MAX,
            },
            bist_rx_done: false,
            stage: Stage::Afe,
            part: Part::Step(Step::wait(0)),
            failed: false,
        };
        if state.afe_active {
            rate.start_rate();
        } else {
            let afe = rate.control.chip().afe_enable([true; 2], [true; 2]);
            rate.part = Part::Chip(afe);
        }
        while rate.outcome().is_none() && rate.part.next().is_none() {
            rate.advance();
        }
        rate
    }

    /// The control state, as the setting has left it.
    pub(crate) const fn control(&self) -> &Control {
        &self.control
    }

    /// The board's rate state, as the setting has left it.
    pub(crate) const fn state(&self) -> RateState {
        self.state
    }

    /// The outcome, once finished.
    pub(crate) const fn outcome(&self) -> Option<Result<(), RateError>> {
        match self.stage {
            Stage::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// Requests the LMS7002M's rate, with the double pump's SISO DDR flags.
    fn start_rate(&mut self) {
        let siso = self.state.double_pump;
        self.request.flags = RateFlags {
            max_conversion: true,
            extended_cgen: true,
            rx_siso_ddr: siso,
            tx_siso_ddr: siso,
            ..RateFlags::default()
        };
        self.part = Part::Control(self.control.sample_rate(self.request));
        self.stage = Stage::Rate;
    }

    /// Runs `step` next at `stage`.
    fn step(&mut self, stage: Stage, step: Step) {
        self.stage = stage;
        self.part = Part::Step(step);
    }

    /// Runs chip writes next at `stage`.
    fn chip(&mut self, stage: Stage, batch: Batch) {
        self.stage = stage;
        self.part = Part::Chip(batch);
    }

    /// The deserializer flag (`sisosdrflag`): 16 double-pumped, 8 for RX SISO DDR.
    fn deser_flag(&self) -> u16 {
        if self.state.double_pump {
            16
        } else if self.control.lml().rx_siso_ddr {
            8
        } else {
            0
        }
    }

    /// The stage after the current part finished.
    fn advance(&mut self) {
        match self.stage {
            Stage::Afe => self.step(Stage::AfeSettle, Step::wait(10_000)),
            Stage::AfeSettle => {
                self.state.afe_active = true;
                if self.request.rx > SISO_RATE || self.request.tx > SISO_RATE {
                    self.state.double_pump = true;
                }
                self.start_rate();
            }
            Stage::Rate => self.after_rate(),
            Stage::VioBoost => {
                let (buck, voltage) = VDD_BOOST;
                self.pmic_write(Stage::VddBoost, buck, voltage);
            }
            Stage::VddBoost => self.calibrate(),
            Stage::TxDelay => self.after_calibration(),
            Stage::RxPower => self.step(Stage::RxPowerSettle, Step::wait(1000)),
            Stage::RxPowerSettle => {
                self.stage = Stage::StreamUp;
                self.part = Part::StreamUp(self.control.streaming_up_rx(
                    XsdrFrontend,
                    Mac::Both,
                    StreamFlags::default(),
                ));
            }
            Stage::StreamUp => {
                if let Part::StreamUp(op) = self.part {
                    if let Some(Err(error)) = op.outcome() {
                        self.stage = Stage::Done(Err(RateError::Control(error)));
                        return;
                    }
                }
                self.limelight_reset();
            }
            Stage::LimelightReset => self.deser(0),
            Stage::Deser(step) if step < 2 => self.deser(step + 1),
            Stage::Deser(_) => self.start_retry(),
            Stage::LfsrMode => {
                self.sweep.probe = Probe::Lead;
                self.probe_step();
            }
            Stage::Probe => self.probe_advance(),
            Stage::NormalMode => self.finish_calibration(),
            Stage::SelectA => self.bist_wait(),
            Stage::BistWait => {
                self.stage = Stage::BistRx;
                self.part = Part::Bist(self.control.chip().bist_check(Tsp::Rx));
            }
            Stage::BistRx => {
                self.bist_rx_done =
                    matches!(self.part, Part::Bist(check) if check.done() == Some(true));
                self.stage = Stage::BistTx;
                self.part = Part::Bist(self.control.chip().bist_check(Tsp::Tx));
            }
            Stage::BistTx => self.after_bist(),
            Stage::Done(_) => {}
        }
    }

    /// After the LMS7002M's rate: its outcome, the rates stored, then the calibration.
    fn after_rate(&mut self) {
        if let Part::Control(op) = self.part {
            if let Some(Err(error)) = op.outcome() {
                self.stage = Stage::Done(Err(RateError::Control(error)));
                return;
            }
        }
        self.state.rx = self.request.rx;
        self.state.tx = self.request.tx;
        self.was_running = self.control.rx_running().contains(&true);
        if !self.gateware.tx_mmcm {
            self.step(Stage::TxDelay, Step::fpga(TX_DEFAULT_DELAY));
            return;
        }
        if self.state.rx > BOOST_RATE || self.state.tx > BOOST_RATE {
            let (buck, voltage) = VIO_BOOST;
            self.pmic_write(Stage::VioBoost, buck, voltage);
            return;
        }
        self.calibrate();
    }

    /// Writes a PMIC rail (`lp8758_vout_set`).
    fn pmic_write(&mut self, stage: Stage, buck: Buck, voltage: BuckVoltage) {
        let write = RegisterAccess::write(self.pmic, voltage.entry_at(buck));
        self.step(stage, Step::i2c(write, lp8758::NAME));
    }

    /// The calibration with a TX MMCM: RX streamed up unless it runs (the RX MMCM, which
    /// this gateware lacks, would be configured next), then the `LimeLight` reset.
    fn calibrate(&mut self) {
        if self.control.rx_running().contains(&true) {
            self.limelight_reset();
            return;
        }
        let pins = LMS_LDO_ENABLE | LMS_RESET_RELEASE | LMS_RX_ENABLE | LMS_TX_ENABLE;
        self.step(Stage::RxPower, Step::fpga(Gpo::LmsPower.write(pins)));
    }

    /// The `LimeLight` reset pulse.
    fn limelight_reset(&mut self) {
        let batch = self.control.chip().limelight_reset();
        self.chip(Stage::LimelightReset, batch);
    }

    /// `_xsdr_rxserdes_reset`'s step `step`.
    fn deser(&mut self, step: u8) {
        let flag = self.deser_flag();
        let action = match step {
            0 => Step::fpga(RxPhyBank::Control.access(true, 0, DESER_RESET | flag)),
            1 => Step::wait(10),
            _ => Step::fpga(RxPhyBank::Control.access(true, 0, flag)),
        };
        self.step(Stage::Deser(step), action);
    }

    /// A retry: the RX FIFO to the LFSR pattern and the sweep from phase 1.
    fn start_retry(&mut self) {
        let sweep = &mut self.sweep;
        sweep.kept = 0;
        sweep.clean = (i32::from(PHASES), 0);
        sweep.last = -1;
        sweep.least = u64::MAX;
        sweep.phase = 1;
        self.stage = Stage::LfsrMode;
        self.part = Part::Control(self.control.set_lml_rx_mode(true, false));
    }

    /// Installs the current probe step.
    fn probe_step(&mut self) {
        let flag = self.deser_flag();
        let step = match self.sweep.probe {
            Probe::FifoReset => {
                let batch = self.control.chip().limelight_fifo_reset(true, true);
                self.chip(Stage::Probe, batch);
                return;
            }
            Probe::DeserReset => Step::fpga(RxPhyBank::Control.access(true, 0, DESER_RESET | flag)),
            Probe::Lead | Probe::DeserWait | Probe::Settle => Step::wait(10),
            Probe::DeserRun => Step::fpga(RxPhyBank::Control.access(true, 0, flag)),
            Probe::Checker(0) => Step::fpga(RxPhyBank::IqAbCheck.access(true, 0, 0)),
            Probe::Checker(1) => Step::fpga(RxPhyBank::LfsrCheck.access(true, 0, 0x0f)),
            Probe::Checker(_) => Step::fpga(RxPhyBank::LfsrCheck.access(true, 0, 0xf0)),
            Probe::CheckWait(_) => Step::wait(100),
            Probe::Select(_, counter) => {
                Step::fpga(RxPhyBank::LfsrCheck.access(false, BER | counter, 0))
            }
            Probe::SelectWait(..) => Step::wait(1),
            Probe::Read(..) => Step::fpga(FpgaAccess::Read { reg: REG_CFG_PHY_0 }),
        };
        self.step(Stage::Probe, step);
    }

    /// The probe's next step, or its verdict.
    fn probe_advance(&mut self) {
        let next = match self.sweep.probe {
            Probe::Lead => Probe::FifoReset,
            Probe::FifoReset => Probe::DeserReset,
            Probe::DeserReset => Probe::DeserWait,
            Probe::DeserWait => Probe::DeserRun,
            Probe::DeserRun => Probe::Settle,
            Probe::Settle => Probe::Checker(0),
            Probe::Checker(n) if n < 2 => Probe::Checker(n + 1),
            Probe::Checker(_) => Probe::CheckWait(0),
            Probe::CheckWait(check) => Probe::Select(check, 0),
            Probe::Select(check, counter) => Probe::SelectWait(check, counter),
            Probe::SelectWait(check, counter) => Probe::Read(check, counter),
            Probe::Read(check, counter) => {
                if let Part::Step(step) = self.part {
                    self.sweep.errors[usize::from(counter)] = step.value().unwrap_or_default();
                }
                if counter < 3 {
                    Probe::Select(check, counter + 1)
                } else {
                    let clean = self.checked();
                    if clean && check + 1 < CHECKS {
                        Probe::CheckWait(check + 1)
                    } else {
                        self.probe_done(check + u8::from(clean));
                        return;
                    }
                }
            }
        };
        self.sweep.probe = next;
        self.probe_step();
    }

    /// Whether the counters read clean (`noerrors_v4`, or `noerrors_v2` double-pumped),
    /// computing the badness libusdr does: the counters' squares in 32-bit arithmetic.
    fn checked(&mut self) -> bool {
        let counted = if self.state.double_pump { 2 } else { 4 };
        let errors = &self.sweep.errors[..counted];
        let badness = errors.iter().fold(0u32, |sum, &count| {
            sum.wrapping_add(count.wrapping_mul(count))
        });
        self.sweep.badness = u64::from(badness);
        errors.iter().all(|&count| count == 0)
    }

    /// A probe finished after `checks` checks (`w`).
    fn probe_done(&mut self, checks: u8) {
        // `badness *= 1.0 * check_to / (w + 1)`.
        let scale = f64::from(CHECKS) / f64::from(checks + 1);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "libusdr's double rescale of a 32-bit sum, converted back to unsigned"
        )]
        let rescaled = (self.sweep.badness as f64 * scale) as u64;
        if self.sweep.phase == 0 {
            self.rx_badness = rescaled;
            self.after_best_effort();
            return;
        }
        let phase = i32::from(self.sweep.phase);
        // libusdr re-evaluates the last counters: their badness, unscaled, is what the
        // sweep compares.
        let clean = self.checked();
        let sweep = &mut self.sweep;
        sweep.last = phase;
        sweep.last_clean = clean;
        let mut ended = false;
        if sweep.last_clean {
            sweep.kept = phase;
            sweep.clean.0 = sweep.clean.0.min(phase);
            sweep.clean.1 = sweep.clean.1.max(phase);
            let spread = match sweep.retry {
                0 => 8,
                1 => 4,
                2 => 1,
                _ => 0,
            };
            ended = sweep.clean.1 - sweep.clean.0 >= spread;
        } else if sweep.clean.1 >= sweep.clean.0 {
            ended = true;
        }
        if !ended && sweep.least > sweep.badness {
            sweep.least = sweep.badness;
            sweep.kept = phase;
        }
        if !ended && sweep.phase + 1 < PHASES {
            sweep.phase += 1;
            sweep.probe = Probe::Lead;
            self.probe_step();
            return;
        }
        self.after_sweep();
    }

    /// After a sweep: the middle of the clean band kept; done if the last probe was it
    /// and clean, else a best-effort probe of the kept phase.
    fn after_sweep(&mut self) {
        let sweep = &mut self.sweep;
        if sweep.clean.1 >= sweep.clean.0 {
            sweep.kept = (sweep.clean.1 + sweep.clean.0) / 2;
        }
        if sweep.last == sweep.kept && sweep.last_clean {
            self.rx_badness = 0;
            self.after_calibration();
            return;
        }
        // The best-effort probe: no lead wait (the RX MMCM it would set is absent).
        sweep.phase = 0;
        sweep.probe = Probe::FifoReset;
        self.probe_step();
    }

    /// After the best-effort probe: done when clean, else the next retry.
    fn after_best_effort(&mut self) {
        if self.rx_badness == 0 || self.sweep.retry + 1 >= RETRIES {
            self.after_calibration();
            return;
        }
        self.sweep.retry += 1;
        self.start_retry();
    }

    /// `no_tx`: the RX FIFO back to the processor when RX ran before the call.
    fn after_calibration(&mut self) {
        if self.was_running {
            self.stage = Stage::NormalMode;
            self.part = Part::Control(self.control.set_lml_rx_mode(false, false));
            return;
        }
        self.finish_calibration();
    }

    /// The calibration's verdict, then bank A selected.
    fn finish_calibration(&mut self) {
        if self.rx_badness > SEVERE {
            self.stage = Stage::Done(Err(RateError::Calibration));
            return;
        }
        let batch = self.control.chip_mut().mac_set(Mac::A);
        self.chip(Stage::SelectA, batch);
    }

    /// The 25 ms before the self-test checks.
    fn bist_wait(&mut self) {
        self.step(Stage::BistWait, Step::wait(25_000));
    }

    /// After both checks: again until both processors are done.
    fn after_bist(&mut self) {
        let done = |part: Part| match part {
            Part::Bist(check) => check.done().unwrap_or(false),
            _ => false,
        };
        // The RX check's result was taken when the TX check replaced it.
        if self.bist_rx_done && done(self.part) {
            // With TX running or RX A stopped, libusdr checks the TX/RX quality next; on
            // this path RX has just been streamed up and TX never runs, so it does not.
            self.stage = Stage::Done(Ok(()));
            return;
        }
        self.bist_wait();
    }
}

impl Sequence for SetSampleRate {
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
        let completed = self.part.complete(result);
        match self.part {
            Part::Control(op) => self.control = *op.control(),
            Part::StreamUp(op) => self.control = *op.control(),
            Part::Chip(_) | Part::Step(_) | Part::Bist(_) => {}
        }
        match completed {
            Ok(()) => {}
            // `lms7002m_afe_enable`'s result is ignored on the way to the rate.
            Err(_) if matches!(self.stage, Stage::Afe) => self.advance(),
            Err(failure) => {
                self.failed = true;
                return Err(failure);
            }
        }
        while self.outcome().is_none() && self.part.next().is_none() {
            self.advance();
        }
        Ok(())
    }
}
