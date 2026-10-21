//! The TX half of the `LimeLight` link calibration on gateware with a TX MMCM, when TX
//! runs: `_xsdr_calibrate_lml`'s TX phase sweep (`device/m2_lm7_1/xsdr_ctrl.c`), after its
//! RX sweep chose the RX phase.
//!
//! The FPGA's TX PHY sends its LFSR pattern; the LMS7002M loops it back digitally to the RX
//! FIFO (`XSDR_LMLRX_DIGLOOPBACK`) and the RX PHY's checkers count errors. For each TX
//! phase, 1 to 252, the TX MMCM is reprogrammed and the pattern checked
//! (`_xsdr_calibrate_txlfsr_check`); at the first clean phase the A/B and I/Q alignment
//! is checked with the generator's A/B-I/Q table, the lane selection rotated until it
//! holds. Up to eight sweeps, each after toggling the TX FIFO clocks' inversion, the
//! later ones rechecking the RX link at each phase; failing those, the phase that read
//! best is restored. Then the generators and checkers stop, the RX FIFO returns to the
//! processor and the processors are reset and self-tested.
//!
//! libusdr's `GLOBAL_RESYNC` (a `goto` back to the RX sweep after more than 30 failed RX
//! rechecks) cannot happen: its count is reset after the sixth, so it is not ported.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use lms7002m::ctrl::{Control, Operation};
use lms7002m::{Batch, Mac, Tsp};

use super::fpga::{REG_CFG_PHY_0, RxPhyBank, TxPhy};
use super::mmcm::{ConfigureMmcm, MmcmCall, MmcmLink, MmcmPlan, MmcmUnlocked};
use super::step::Step;

/// TX phases swept: 1 to 4 * 63 (`ph`).
const PHASES: u16 = 4 * 63 + 1;
/// Sweeps, each with the other TX clock edge (`rty`).
const SWEEPS: u8 = 8;
/// Checks per TX phase in a sweep (`check_txto`).
const SWEEP_CHECKS: u8 = 35;
/// Checks of the restored phase, and of the RX link (`check_to`).
const CHECKS: u8 = 10;
/// A/B-I/Q alignment attempts (`g`).
const ALIGNS: u8 = 24;
/// The attempt at which libusdr gives the alignment up as good (its "fixup for now").
const ALIGN_GIVE_UP: u8 = 20;
/// Failed RX rechecks retried at the same phase before going on.
const RX_RETRIES: u8 = 6;
/// The LFSR counters' read selector: `LFSR_CNTR_BER` in bits 3:2.
const BER: u8 = 2 << 2;
/// The I/Q-swap counter's read selector, `LFSR_CNTR_IQS << 2`.
const IQS: u8 = 3 << 2;
/// The deserializer's reset word, bits 2:0 of `0x80000007` as the 16-bit value.
const DESER_RESET: u16 = 0x0007;
/// Above this TX rate libusdr gives up an alignment late with little badness.
const FAST_TX: u32 = 80_000_000;
/// `_xsdr_txserdes_reset`'s SISO DDR flag (`sisosdrflag`): 16 double-pumped or for TX SISO
/// DDR.
pub(crate) const fn tx_serdes_flag(double_pump: bool, tx_siso_ddr: bool) -> u32 {
    if double_pump || tx_siso_ddr { 16 } else { 0 }
}

/// The I/Q lane selections tried in turn (`msk`).
const IQ_SELECTIONS: [u32; 4] = [0b1100, 0b0110, 0b0011, 0b1001];
/// The SISO DDR I/Q lane selection.
const SISO_IQ_SELECTION: u32 = 0b1010;
/// The A/B-I/Q check pattern's four words, pushed 32 times (`xsdr_phy_load_iqab_check`):
/// channel A, B, A, B.
const IQAB_PATTERN: [(TxPhy, u32); 4] = [
    (TxPhy::IqAbPushA, 0x00bb_e441),
    (TxPhy::IqAbPushB, 0x005a_5969),
    (TxPhy::IqAbPushA, 0x00c4_2676),
    (TxPhy::IqAbPushB, 0x00f0_f356),
];
/// The pattern's rounds.
const IQAB_ROUNDS: u8 = 32;

/// What the calibration found: libusdr's `tx_badness` and `tx_iqerrs`, which the link's
/// verdict reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TxVerdict {
    /// The LFSR errors' badness at the phase kept.
    pub(crate) badness: u64,

    /// The I/Q swaps counted there.
    pub(crate) iq_errors: u32,
}

/// Why the calibration stopped: the TX MMCM did not lock (libusdr's `-EIO`), or a phase
/// asked a configuration [`MmcmPlan::new`] refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxCalError {
    /// The MMCM did not lock.
    Unlocked,

    /// The MMCM cannot be configured for the link.
    Refused,
}

/// The RX FIFO's source (`XSDR_LMLRX_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RxSource {
    /// The RX processor.
    Normal,
    /// The LFSR pattern.
    Lfsr,
    /// The TX FIFO, the digital loopback.
    Loopback,
}

/// One action of the calibration, built when reached.
#[derive(Clone, Copy, Debug)]
enum Item {
    /// A TX PHY write.
    Tx(TxPhy, u32),
    /// An RX PHY request (`xsdr_phy_rx_reg`): write flag, address, value.
    Rx(RxPhyBank, bool, u8, u16),
    /// A wait, in microseconds.
    Wait(u64),
    /// A read of the RX PHY port: counter 0 to 3, or 4 for the I/Q swaps.
    Read(u8),
    /// The A/B-I/Q check pattern.
    Pattern,
    /// Both `LimeLight` FIFOs reset.
    FifoReset,
    /// The `LimeLight` reset.
    LimelightReset,
    /// The TX FIFO clocks' inversion toggled.
    ToggleTxClocks,
    /// The RX FIFO's source.
    Source(RxSource),
    /// The TX MMCM at this TX phase and offset, the RX phase kept.
    Mmcm {
        /// The TX phase.
        tx_phase: u16,
        /// The offset.
        offset: u8,
    },
    /// Both banks selected.
    BothBanks,
    /// A processor reset.
    TspReset(Tsp),
    /// A processor's self-test started.
    Bist(Tsp),
}

/// What follows once the queued items have run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resume {
    /// The sweep's next phase.
    Phase,
    /// The phase's MMCM is set: recheck RX, or check TX.
    PhaseSet,
    /// RX recheck `check` read.
    RxCheck(u8),
    /// TX check `check` read; the restore's when `restore`.
    TxCheck {
        /// The check.
        check: u8,
        /// The restored phase's check.
        restore: bool,
    },
    /// The TX check's I/Q-swap count read.
    TxSwaps {
        /// The restored phase's check.
        restore: bool,
    },
    /// Alignment attempt `align`'s I/Q-swap count read.
    Aligned(u8),
    /// Alignment attempt `align`'s lane rotation done.
    Rotated(u8),
    /// Alignment attempt `align`'s LFSR recheck read.
    Rechecked(u8),
    /// The generator back on the LFSR after a phase whose alignment failed.
    Misaligned,
    /// The TX FIFO clocks toggled: the next sweep.
    Toggled,
    /// The restored phase's I/Q-swap count read.
    Restored,
    /// The generators, checkers and processors done.
    Finished,
}

/// The queue's capacity: the longest run of items between decisions is 26 (a TX check's
/// set-up and first check after an RX recheck or the restored phase's MMCM).
const QUEUE: usize = 32;

/// The part running for the current item.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "the portable tree allocates nothing; one part runs at a time"
)]
enum Part {
    /// Nothing yet.
    Idle,
    /// One board action.
    Step(Step),
    /// Chip writes.
    Chip(Batch),
    /// A control-code operation.
    Control(Operation),
    /// The TX MMCM's configuration.
    Mmcm(ConfigureMmcm),
    /// The pattern: round and word next.
    Pattern(u8, u8),
}

/// `_xsdr_calibrate_lml`'s TX phase calibration.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TxCalibration {
    /// The LMS7002M control state.
    control: Control,

    /// The link the MMCM is computed for; its `LimeLight` configuration is the control's.
    link: MmcmLink,

    /// The RX phase the RX sweep kept (`lmlcal_rx_phase`).
    rx_phase: u16,

    /// The RX retries' VCO divider steps (`g_clk_reduce`).
    clock_reduce: u8,

    /// The TX rate (`s_txrate`).
    tx_rate: u32,

    /// The sweep (`rty`).
    sweep: u8,

    /// The phase (`ph`).
    phase: u16,

    /// Later sweeps recheck RX at each phase (`check_rx`).
    check_rx: bool,

    /// Failed RX rechecks in a row (`failed_rxcnt`).
    failed_rx: u8,

    /// The phase's last badness (`badness`).
    badness: u64,

    /// The least badness seen, and where (`badness_m`, `phase_m`, `ph_ty_m`).
    best: (u64, u16, u8),

    /// The counters as last read (`errs`).
    errors: [u32; 4],

    /// The last I/Q-swap count (`iqserrs`).
    iq_errors: u32,

    /// Items to run, the first `len`, from `at`.
    queue: [Item; QUEUE],

    /// Items queued.
    len: usize,

    /// The item running.
    at: usize,

    /// Its part.
    part: Part,

    /// What follows the queue.
    resume: Resume,

    /// The outcome, once finished.
    outcome: Option<Result<TxVerdict, TxCalError>>,

    /// A bus access failed.
    failed: bool,
}

impl TxCalibration {
    /// Starts the TX calibration with the RX phase `rx_phase` the RX sweep kept, the RX
    /// retries' `clock_reduce` and the TX rate `tx_rate`, on `link`.
    pub(crate) fn new(
        control: &Control,
        link: MmcmLink,
        rx_phase: u16,
        clock_reduce: u8,
        tx_rate: u32,
    ) -> Self {
        let mut calibration = Self {
            control: *control,
            link,
            rx_phase,
            clock_reduce,
            tx_rate,
            sweep: 0,
            phase: 1,
            check_rx: false,
            failed_rx: 0,
            badness: u64::MAX,
            best: (u64::MAX, 0, 0),
            errors: [u32::MAX; 4],
            iq_errors: 0,
            queue: [Item::Wait(0); QUEUE],
            len: 0,
            at: 0,
            part: Part::Idle,
            resume: Resume::Phase,
            outcome: None,
            failed: false,
        };
        calibration.push(Item::Pattern);
        calibration.push(Item::Source(RxSource::Loopback));
        calibration.generator(true, true);
        calibration.lfsr_checker(true);
        calibration.settle();
        calibration
    }

    /// The control state, as the calibration has left it.
    pub(crate) const fn control(&self) -> &Control {
        &self.control
    }

    /// The outcome, once finished.
    pub(crate) const fn outcome(&self) -> Option<Result<TxVerdict, TxCalError>> {
        self.outcome
    }

    /// Queues `item`.
    fn push(&mut self, item: Item) {
        self.queue[self.len] = item;
        self.len += 1;
    }

    /// `xsdr_phy_en_lfsr_generator_mimo(d, run, lfsr)`.
    fn generator(&mut self, run: bool, lfsr: bool) {
        self.push(Item::Tx(TxPhy::LfsrGenerator, 0));
        self.push(Item::Wait(1));
        self.push(Item::Tx(TxPhy::IqAbGenerator, u32::from(!lfsr)));
        self.push(Item::Tx(TxPhy::LfsrGenerator, u32::from(run)));
    }

    /// `xsdr_phy_en_lfsr_checker_mimo(d, on)`.
    fn lfsr_checker(&mut self, on: bool) {
        self.push(Item::Rx(RxPhyBank::IqAbCheck, true, 0, 0));
        self.push(Item::Rx(RxPhyBank::LfsrCheck, true, 0, 0x0f));
        if on {
            self.push(Item::Rx(RxPhyBank::LfsrCheck, true, 0, 0xf0));
        }
    }

    /// `xsdr_phy_en_iqab_checker_mimo(d, true)`.
    fn iqab_checker(&mut self) {
        self.push(Item::Rx(RxPhyBank::LfsrCheck, true, 0, 0x0f));
        self.push(Item::Rx(RxPhyBank::IqAbCheck, true, 0, 0));
        self.push(Item::Wait(1));
        self.push(Item::Rx(RxPhyBank::IqAbCheck, true, 0, 1));
    }

    /// `xsdr_phy_lfsr_mimo_state(d, LFSR_CNTR_BER, errs)`: each counter selected, then read.
    fn read_errors(&mut self) {
        for counter in 0..4 {
            self.push(Item::Rx(RxPhyBank::LfsrCheck, false, BER | counter, 0));
            self.push(Item::Wait(1));
            self.push(Item::Read(counter));
        }
    }

    /// `xsdr_phy_lfsr_mimo_state_s(d, LFSR_CNTR_IQS << 2, ..)`.
    fn read_swaps(&mut self) {
        self.push(Item::Rx(RxPhyBank::LfsrCheck, false, IQS, 0));
        self.push(Item::Wait(1));
        self.push(Item::Read(4));
    }

    /// `_xsdr_rxserdes_reset`.
    fn rx_deserializer_reset(&mut self) {
        let flag = if self.link.double_pump {
            16
        } else if self.control.lml().rx_siso_ddr {
            8
        } else {
            0
        };
        self.push(Item::Rx(RxPhyBank::Control, true, 0, DESER_RESET | flag));
        self.push(Item::Wait(10));
        self.push(Item::Rx(RxPhyBank::Control, true, 0, flag));
    }

    /// `_xsdr_calibrate_txlfsr_check`'s set-up: the serializers and FIFOs reset, the
    /// checkers on.
    fn tx_check(&mut self, restore: bool) {
        let flag = tx_serdes_flag(self.link.double_pump, self.control.lml().tx_siso_ddr);
        self.push(Item::Tx(TxPhy::PortControl, flag));
        self.push(Item::Wait(1));
        self.push(Item::Tx(TxPhy::PortControl, 1 | flag));
        self.push(Item::Wait(1));
        self.push(Item::FifoReset);
        self.rx_deserializer_reset();
        self.push(Item::Wait(20));
        self.lfsr_checker(true);
        self.check(Resume::TxCheck { check: 0, restore });
    }

    /// One LFSR check: 100 µs, then the counters, then `resume`.
    fn check(&mut self, resume: Resume) {
        self.push(Item::Wait(100));
        self.read_errors();
        self.resume = resume;
    }

    /// Whether the counters read clean (`noerrors_v4`, `noerrors_v2` double-pumped),
    /// leaving their badness, the squares summed in 32-bit arithmetic, in `badness`.
    fn clean(&mut self) -> bool {
        let counted = if self.link.double_pump { 2 } else { 4 };
        let errors = &self.errors[..counted];
        let sum = errors.iter().fold(0u32, |sum, &count| {
            sum.wrapping_add(count.wrapping_mul(count))
        });
        self.badness = u64::from(sum);
        errors.iter().all(|&count| count == 0)
    }

    /// `badness *= 1.0 * checks / (made + 1)`.
    fn rescaled(badness: u64, checks: u8, made: u8) -> u64 {
        let scale = f64::from(checks) / f64::from(made + 1);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "libusdr's double rescale, converted back to unsigned"
        )]
        let rescaled = (badness as f64 * scale) as u64;
        rescaled
    }

    /// The alignment failed at this sweep: libusdr's `(rty == 0 && iqserrs != 0) ||
    /// iqserrs > 40`.
    const fn misaligned(&self) -> bool {
        (self.sweep == 0 && self.iq_errors != 0) || self.iq_errors > 40
    }

    /// The sweep's phase: past the last, the clocks toggled; else its MMCM.
    fn phase(&mut self) {
        if self.phase >= PHASES {
            self.push(Item::ToggleTxClocks);
            self.resume = Resume::Toggled;
            return;
        }
        self.push(Item::Mmcm {
            tx_phase: self.phase,
            offset: 0,
        });
        self.resume = Resume::PhaseSet;
    }

    /// The phase's checks: RX first in later sweeps.
    fn phase_set(&mut self) {
        if !self.check_rx {
            self.tx_check(false);
            return;
        }
        self.push(Item::Source(RxSource::Lfsr));
        self.push(Item::FifoReset);
        self.rx_deserializer_reset();
        self.push(Item::Wait(10));
        self.lfsr_checker(true);
        self.check(Resume::RxCheck(0));
    }

    /// After RX recheck `check`.
    fn rx_checked(&mut self, check: u8) {
        if self.clean() {
            if check + 1 < CHECKS {
                self.check(Resume::RxCheck(check + 1));
                return;
            }
        } else if self.badness > 20 {
            self.failed_rx += 1;
            if self.failed_rx < RX_RETRIES {
                // `ph--; continue`: the same phase again.
                self.phase();
                return;
            }
        }
        self.push(Item::Source(RxSource::Loopback));
        self.failed_rx = 0;
        self.tx_check(false);
    }

    /// After TX check `check`.
    fn tx_checked(&mut self, check: u8, restore: bool) {
        let checks = if restore { CHECKS } else { SWEEP_CHECKS };
        let clean = self.clean();
        if clean && check + 1 < checks {
            self.check(Resume::TxCheck {
                check: check + 1,
                restore,
            });
            return;
        }
        let made = check + u8::from(clean);
        self.badness = Self::rescaled(self.badness, checks, made);
        self.read_swaps();
        self.resume = Resume::TxSwaps { restore };
    }

    /// After the TX check's I/Q-swap count: the restore's alignment check, or the
    /// sweep's verdict on the phase.
    fn tx_swaps(&mut self, restore: bool) {
        if restore {
            self.align();
            self.resume = Resume::Restored;
            return;
        }
        // libusdr's `noerrors` replaces the rescaled badness with the counters' own.
        let clean = self.clean();
        if clean || (self.sweep > 1 && self.badness < 20) {
            self.best.1 = self.phase;
            self.alignment(0);
            return;
        }
        if self.best.0 > self.badness {
            self.best = (self.badness, self.phase, self.sweep);
        }
        self.phase += 1;
        self.phase();
    }

    /// The A/B and I/Q alignment check: the generator on its table, the checker on, the
    /// swaps counted.
    fn align(&mut self) {
        self.generator(true, false);
        self.push(Item::Wait(10));
        self.iqab_checker();
        self.push(Item::Wait(1000));
        self.read_swaps();
    }

    /// Alignment attempt `align`, or the verdict past the last.
    fn alignment(&mut self, align: u8) {
        if align >= ALIGNS {
            self.aligned_phase();
            return;
        }
        self.align();
        self.resume = Resume::Aligned(align);
    }

    /// After attempt `align`'s count.
    fn aligned(&mut self, align: u8) {
        if self.misaligned() {
            let selection = if !self.link.double_pump && self.control.lml().tx_siso_ddr {
                SISO_IQ_SELECTION
            } else {
                IQ_SELECTIONS[usize::from(align + 1) % IQ_SELECTIONS.len()]
            };
            self.push(Item::Tx(TxPhy::IqSelect, selection));
            self.push(Item::Wait(10));
            self.push(Item::LimelightReset);
            self.push(Item::Wait(10));
            self.resume = Resume::Rotated(align);
            return;
        }
        if align == 0 {
            self.alignment(1);
            return;
        }
        // A count of 0 can hide a lost pattern: recheck the LFSR.
        self.generator(true, true);
        self.push(Item::Wait(10));
        self.lfsr_checker(true);
        self.push(Item::Wait(100));
        self.read_errors();
        self.resume = Resume::Rechecked(align);
    }

    /// After attempt `align`'s lane rotation.
    fn rotated(&mut self, align: u8) {
        if align == ALIGN_GIVE_UP {
            self.iq_errors = 0;
            self.aligned_phase();
            return;
        }
        self.alignment(align + 1);
    }

    /// After attempt `align`'s LFSR recheck.
    fn rechecked(&mut self, align: u8) {
        if self.clean() {
            self.aligned_phase();
            return;
        }
        self.iq_errors = 500;
        if align > 18 && self.badness < 100 && self.tx_rate > FAST_TX {
            self.aligned_phase();
            return;
        }
        self.alignment(align + 1);
    }

    /// After the alignment: the next phase if it failed, else calibrated.
    fn aligned_phase(&mut self) {
        if self.misaligned() {
            self.generator(true, true);
            self.resume = Resume::Misaligned;
            return;
        }
        self.finish();
    }

    /// The generators and checkers stopped, the RX FIFO back on the processor, both banks'
    /// processors reset and self-tested.
    fn finish(&mut self) {
        self.generator(false, false);
        self.lfsr_checker(false);
        self.push(Item::Source(RxSource::Normal));
        self.push(Item::BothBanks);
        self.push(Item::TspReset(Tsp::Tx));
        self.push(Item::TspReset(Tsp::Rx));
        self.push(Item::Bist(Tsp::Tx));
        self.push(Item::Bist(Tsp::Rx));
        self.resume = Resume::Finished;
    }

    /// The next sweep with the other clock edge, or the best phase restored.
    fn toggled(&mut self) {
        self.check_rx = true;
        self.sweep += 1;
        if self.sweep < SWEEPS {
            self.phase = 1;
            self.phase();
            return;
        }
        let (_, phase, sweep) = self.best;
        self.push(Item::Mmcm {
            tx_phase: phase,
            offset: sweep,
        });
        self.tx_check(true);
    }

    /// Decides what follows the queue.
    fn resume(&mut self) {
        self.len = 0;
        self.at = 0;
        match self.resume {
            Resume::Phase => self.phase(),
            Resume::PhaseSet => self.phase_set(),
            Resume::RxCheck(check) => self.rx_checked(check),
            Resume::TxCheck { check, restore } => self.tx_checked(check, restore),
            Resume::TxSwaps { restore } => self.tx_swaps(restore),
            Resume::Aligned(align) => self.aligned(align),
            Resume::Rotated(align) => self.rotated(align),
            Resume::Rechecked(align) => self.rechecked(align),
            Resume::Misaligned => {
                self.phase += 1;
                self.phase();
            }
            Resume::Toggled => self.toggled(),
            Resume::Restored => self.finish(),
            Resume::Finished => {
                // The aligned phase's badness, or the restored phase's rescaled one.
                self.outcome = Some(Ok(TxVerdict {
                    badness: self.badness,
                    iq_errors: self.iq_errors,
                }));
            }
        }
    }

    /// Starts the item at `at`.
    fn start_item(&mut self) {
        self.part = match self.queue[self.at] {
            Item::Tx(register, value) => Part::Step(Step::fpga(register.write(value))),
            Item::Rx(bank, write, address, value) => {
                Part::Step(Step::fpga(bank.access(write, address, value)))
            }
            Item::Wait(micros) => Part::Step(Step::wait(micros)),
            Item::Read(_) => Part::Step(Step::fpga(FpgaAccess::Read { reg: REG_CFG_PHY_0 })),
            Item::Pattern => Part::Pattern(0, 0),
            Item::FifoReset => Part::Chip(self.control.chip().limelight_fifo_reset(true, true)),
            Item::LimelightReset => Part::Chip(self.control.chip().limelight_reset()),
            Item::ToggleTxClocks => Part::Chip(self.control.chip().limelight_toggle_tx_clocks()),
            Item::Source(source) => {
                let (lfsr, loopback) = match source {
                    RxSource::Normal => (false, false),
                    RxSource::Lfsr => (true, false),
                    RxSource::Loopback => (false, true),
                };
                Part::Control(self.control.set_lml_rx_mode(lfsr, loopback))
            }
            Item::Mmcm { tx_phase, offset } => {
                let link = MmcmLink {
                    lml: self.control.lml(),
                    ..self.link
                };
                let call = MmcmCall {
                    rx_phase: self.rx_phase,
                    tx_phase,
                    phase_offset: offset,
                    clock_reduce: self.clock_reduce,
                };
                if let Ok(plan) = MmcmPlan::new(&link, call) {
                    Part::Mmcm(ConfigureMmcm::new(&plan))
                } else {
                    self.outcome = Some(Err(TxCalError::Refused));
                    Part::Idle
                }
            }
            Item::BothBanks => Part::Chip(self.control.chip_mut().mac_set(Mac::Both)),
            Item::TspReset(tsp) => Part::Chip(self.control.chip().tsp_reset(tsp)),
            Item::Bist(tsp) => Part::Chip(self.control.chip().tsp_bist_start(tsp)),
        };
    }

    /// The part's next action.
    fn part_next(&self) -> Option<SequenceAction> {
        match &self.part {
            Part::Idle => None,
            Part::Step(step) => step.next(),
            Part::Chip(batch) => batch.next(),
            Part::Control(op) => op.next(),
            Part::Mmcm(configure) => configure.next(),
            &Part::Pattern(round, word) => (round < IQAB_ROUNDS).then(|| {
                let (register, value) = IQAB_PATTERN[usize::from(word)];
                SequenceAction::Fpga(register.write(value))
            }),
        }
    }

    /// Takes the finished part's result, then runs items until one has an action or the
    /// calibration is done.
    fn settle(&mut self) {
        loop {
            if self.outcome.is_some() || self.part_next().is_some() {
                return;
            }
            match self.part {
                Part::Step(step) => {
                    if let Item::Read(slot) = self.queue[self.at] {
                        let value = step.value().unwrap_or_default();
                        match self.errors.get_mut(usize::from(slot)) {
                            Some(counter) => *counter = value,
                            None => self.iq_errors = value,
                        }
                    }
                }
                Part::Mmcm(configure) => {
                    if configure.outcome() == Some(Err(MmcmUnlocked)) {
                        self.outcome = Some(Err(TxCalError::Unlocked));
                        return;
                    }
                }
                Part::Idle | Part::Chip(_) | Part::Control(_) | Part::Pattern(..) => {}
            }
            if !matches!(self.part, Part::Idle) {
                self.at += 1;
            }
            self.part = Part::Idle;
            if self.at >= self.len {
                self.resume();
                if self.outcome.is_some() {
                    return;
                }
            }
            if self.at < self.len {
                self.start_item();
            }
        }
    }
}

impl Sequence for TxCalibration {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed || self.outcome.is_some() {
            return None;
        }
        self.part_next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.next().is_none() {
            return Ok(());
        }
        let completed = match &mut self.part {
            Part::Idle => Ok(()),
            Part::Step(step) => step.complete(result),
            Part::Chip(batch) => batch.complete(result),
            Part::Control(op) => {
                let completed = op.complete(result);
                self.control = *op.control();
                completed
            }
            Part::Mmcm(configure) => configure.complete(result),
            Part::Pattern(round, word) => {
                let (register, value) = IQAB_PATTERN[usize::from(*word)];
                let completed = Step::fpga(register.write(value)).complete(result);
                *word += 1;
                if usize::from(*word) == IQAB_PATTERN.len() {
                    *word = 0;
                    *round += 1;
                }
                completed
            }
        };
        if let Err(failure) = completed {
            self.failed = true;
            return Err(failure);
        }
        self.settle();
        Ok(())
    }
}
