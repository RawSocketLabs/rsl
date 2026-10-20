//! The TX MMCM, which clocks the `LimeLight` link's FPGA side on gateware with a TX MMCM:
//! libusdr's `xsdr_configure_lml_mmcm_tx` (`device/m2_lm7_1/xsdr_ctrl.c`) with TX as the
//! master, its configuration written over the MMCM's dynamic reconfiguration port by
//! `mmcm_init_raw` (`ipblks/xlnx_mmcm.c`), each DRP access served by `m2_lm7_1` itself as
//! TX PHY port traffic (`xsdr_override_drp`).
//!
//! [`MmcmPlan`] is the configuration: the VCO divider for the link's I/O clock, each
//! output's high and low counts, phase and delay, and the feedback divider, which also
//! indexes libusdr's 64-entry lock and filter table (`mmcm_rom`). [`ConfigureMmcm`] writes
//! it: the MMCM held in reset, its power register, the eight outputs, the input divider,
//! the lock and filter settings (each a DRP read, then a write keeping the fields libusdr
//! keeps), the TX I/Q lane selection, the reset released and the lock awaited.
//!
//! libusdr reads `mmcm_rom` past its end when the feedback divider exceeds 63, at TX rates
//! below about 6.35 MS/s, and writes a port's counts before refusing a later port's;
//! [`MmcmPlan::new`] refuses both configurations instead, before anything is written.
//! On the XSDR, whose gateware has no RX or shared MMCM, libusdr's RX-master calls return
//! before any access, so only TX-master configurations exist here.

use core::time::Duration;

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use lms7002m::LmlConfig;
use lms7002m::ctrl::Clocks;

use super::fpga::{self, REG_CFG_PHY_1, TxPhy};
use super::outcome::outcome;

/// The VCO's lowest frequency off the Pro board (`MMCM_VCO_MIN_USP`, which libusdr's XSDR
/// path uses).
const VCO_MIN: u32 = 800_000_000;
/// Its highest (`MMCM_VCO_MAX_USP`).
const VCO_MAX: u32 = 1_600_000_000;

/// The most a divider takes before the doubled feedback of a nomul link: 63.
const DIVIDER_MAX: u32 = 63;
/// The most a counter, phase or delay field takes (`DIG_DIV_MAX`, `DIG_DLY_MAX`).
const FIELD_MAX: u32 = 63;

/// The output libusdr calls `CLKOUT_PORT_0`, the second DRP output (`CLKOUT_PORT_0 = 1`).
const PORT_0: usize = 1;
/// `CLKOUT_PORT_2`: the RX I/O clock.
const PORT_2: usize = 3;
/// `CLKOUT_PORT_5`: the RX fabric clock.
const PORT_5: usize = 0;
/// `CLKOUT_PORT_6`: the TX fabric clock.
const PORT_6: usize = 6;
/// `CLKOUT_PORT_FB`: the feedback.
const PORT_FB: usize = 7;
/// Outputs in DRP order, feedback last (`MAX_MMCM_PORTS`).
const PORTS: usize = 8;

/// The DRP registers `mmcm_init_raw` writes (`enum mmcm_regs`).
mod drp {
    /// The first output's first register (`CLKOUT5_ClkReg1`); output `i` is at `6 + 2i`.
    pub(super) const CLKOUT: u8 = 0x06;
    /// The input divider (`DIVCLK_DivReg`).
    pub(super) const DIVCLK: u8 = 0x16;
    /// The lock settings (`LockReg1` to `LockReg3`).
    pub(super) const LOCK: u8 = 0x18;
    /// The 7-series power register (`PowerRegV7`).
    pub(super) const POWER: u8 = 0x28;
    /// The filter settings (`FiltReg1`, `FiltReg2`).
    pub(super) const FILTER: u8 = 0x4e;
}

/// libusdr's `mmcm_rom`: lock and filter settings by feedback divider, transcribed from
/// the vendored `ipblks/mmcm_rom.h`.
#[expect(
    clippy::unreadable_literal,
    reason = "transcribed digit for digit from the vendored header"
)]
const MMCM_ROM: [u64; 64] = [
    0x3c088ffcfafa401,
    0x2f0bc31be8fa401,
    0x4f0bc31be8fa401,
    0x5b0bc423e8fa401,
    0x770bc5afe8fa401,
    0xd709c73be8fa401,
    0xeb0ac8c7e8fa401,
    0xed0b49cfe8fa401,
    0xf308cb5be8fa401,
    0xe5094ce7e8fa401,
    0xf5094e73e8fa401,
    0xf90a4fff84fa401,
    0xd10b8fff39fa401,
    0xf90b8ffeeefa401,
    0xf90b8ffebcfa401,
    0xf90b8ffe8afa401,
    0xf9084ffe71fa401,
    0xf5084ffe3ffa401,
    0xf5084ffe26fa401,
    0xc1098ffe0dfa401,
    0xc1098ffdf4fa401,
    0xc1098ffddbfa401,
    0x5c098ffdc2fa401,
    0x5c098ffda9fa401,
    0x5c098ffd90fa401,
    0x5c098ffd90fa401,
    0x340a8ffd77fa401,
    0x340a8ffd5efa401,
    0x340a8ffd5efa401,
    0x340a8ffd45fa401,
    0x340a8ffd45fa401,
    0x340b0ffd2cfa401,
    0x340b0ffd2cfa401,
    0x340b0ffd2cfa401,
    0x340b0ffd13fa401,
    0x340b0ffd13fa401,
    0x340b0ffd13fa401,
    0x340b0ffcfafa401,
    0x340b0ffcfafa401,
    0x340b0ffcfafa401,
    0x340b0ffcfafa401,
    0x340b0ffcfafa401,
    0x280b0ffcfafa401,
    0x280b0ffcfafa401,
    0x280b0ffcfafa401,
    0x280b0ffcfafa401,
    0x280b0ffcfafa401,
    0x710b0ffcfafa401,
    0x71088ffcfafa401,
    0x4c088ffcfafa401,
    0x4c088ffcfafa401,
    0x4c088ffcfafa401,
    0x4c088ffcfafa401,
    0x61088ffcfafa401,
    0x61088ffcfafa401,
    0x56088ffcfafa401,
    0x56088ffcfafa401,
    0x56088ffcfafa401,
    0x24088ffcfafa401,
    0x24088ffcfafa401,
    0x24088ffcfafa401,
    0x24088ffcfafa401,
    0x4a088ffcfafa401,
    0x3c088ffcfafa401,
];

/// What one `xsdr_configure_lml_mmcm_tx` call asks beyond the link's clocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MmcmCall {
    /// The RX I/O clock's phase, 1 to 252, or 0 for the default (`rxphase`).
    pub(crate) rx_phase: u16,

    /// The TX fabric clock's phase, likewise (`txphase`).
    pub(crate) tx_phase: u16,

    /// Extra VCO divider steps, at most 2 counted (`txphase_off`).
    pub(crate) phase_offset: u8,

    /// The retries' VCO divider steps so far, 0 to 2 (`g_clk_reduce`).
    pub(crate) clock_reduce: u8,
}

/// The link facts the configuration depends on: the clocks the rate left, the `LimeLight`
/// configuration, the double pump and the gateware's separate `CLKDIV` output.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MmcmLink {
    /// The LMS7002M's clocks (`cgen_clk`, `txcgen_div`, `txtsp_div`).
    pub(crate) clocks: Clocks,

    /// Its `LimeLight` configuration (`lml_mode`).
    pub(crate) lml: LmlConfig,

    /// The link is double-pumped (`dpump`).
    pub(crate) double_pump: bool,

    /// The gateware has a separate `CLKDIV` output (HWID PHY bit 0x80, `sep_clkdiv`).
    pub(crate) separate_clkdiv: bool,
}

/// One output's counts, in VCO cycles (`struct mmcm_port_config_raw`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Port {
    /// Low count (`period_l`).
    low: u32,
    /// High count (`period_h`).
    high: u32,
    /// VCO phase, eighths of a cycle (`phase`).
    phase: u32,
    /// Delay in whole cycles (`delay`).
    delay: u32,
}

impl Port {
    /// A 50 % output dividing by `divider`: the low count takes the odd cycle.
    const fn halves(divider: u32) -> Self {
        Self {
            low: divider.div_ceil(2),
            high: divider / 2,
            phase: 0,
            delay: 0,
        }
    }

    /// High and low both `divider`.
    const fn doubled(divider: u32) -> Self {
        Self {
            low: divider,
            high: divider,
            phase: 0,
            delay: 0,
        }
    }

    /// With its phase and delay at `raw` eighths of a VCO cycle.
    const fn at(self, raw: u32) -> Self {
        Self {
            phase: raw % 8,
            delay: raw / 8,
            ..self
        }
    }

    /// `mmcm_init_raw_clkout`'s check: every field fits its six bits.
    const fn fits(self) -> bool {
        self.high <= FIELD_MAX && self.low <= FIELD_MAX && self.delay <= FIELD_MAX
    }
}

/// Why the TX MMCM cannot be configured as libusdr would: its feedback divider is past
/// `mmcm_rom`'s 64 entries (libusdr reads past the table), or an output's counts or delay
/// are past six bits (libusdr writes the earlier outputs, then returns `-EINVAL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MmcmRefusal;

/// A TX MMCM configuration (`struct mmcm_config_raw` and the VCO divider).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MmcmPlan {
    /// The outputs in DRP order, feedback last.
    ports: [Port; PORTS],

    /// The TX I/Q lane selection written after the outputs.
    iq_select: u32,
}

impl MmcmPlan {
    /// `xsdr_configure_lml_mmcm_tx(d, false, rx_phase, tx_phase, phase_offset)` on `link`.
    ///
    /// # Errors
    ///
    /// [`MmcmRefusal`] where libusdr reads past `mmcm_rom` or refuses an output after
    /// writing the earlier ones; also for clocks that would divide by zero, which no rate
    /// leaves.
    pub(crate) fn new(link: &MmcmLink, call: MmcmCall) -> Result<Self, MmcmRefusal> {
        let MmcmLink {
            clocks,
            lml,
            double_pump,
            separate_clkdiv,
        } = *link;
        let nomul = !double_pump && (lml.tx_siso_ddr || clocks.tx_tsp_div > 1);
        let tx_mclk = clocks
            .cgen
            .checked_div(clocks.tx_converter_div)
            .and_then(|hz| hz.checked_div(u32::from(lml.tx_div)))
            .ok_or(MmcmRefusal)?;
        // Unsigned 32-bit arithmetic, as in C.
        let io_clk = if nomul {
            tx_mclk
        } else {
            tx_mclk.wrapping_mul(2)
        };
        if io_clk == 0 {
            return Err(MmcmRefusal);
        }
        let fitting = |io_clk: u32| VCO_MAX.wrapping_add(io_clk - 1) / io_clk;
        let mut divider =
            fitting(io_clk) + u32::from(call.clock_reduce) + u32::from(call.phase_offset.min(2));
        divider = divider.min(DIVIDER_MAX);
        if divider.wrapping_mul(io_clk) < VCO_MIN {
            if nomul && !separate_clkdiv {
                divider = fitting(io_clk);
                divider += divider % 2;
                divider = divider.min(2 * DIVIDER_MAX);
            }
            if divider < DIVIDER_MAX {
                divider += if (divider + 2).wrapping_mul(io_clk) > VCO_MAX {
                    1
                } else {
                    2
                };
            }
        }

        let half = Port::halves(divider);
        let mut ports = [half; PORTS];
        if separate_clkdiv {
            ports[PORT_0] = Port::doubled(divider);
        }
        if double_pump {
            ports[PORT_5] = Port::doubled(divider);
            ports[PORT_6] = Port::doubled(divider);
        }
        // 90 degrees: half of the 8 * divider eighths of a cycle.
        ports[PORT_6] = ports[PORT_6].at(4 * divider);
        if separate_clkdiv && (call.tx_phase != 0 || call.phase_offset != 0) {
            // libusdr's `tx_override_phase_iq - 1` without an override wraps.
            let raw = if call.tx_phase != 0 {
                u32::from(call.phase_offset)
            } else {
                u32::MAX
            };
            ports[PORT_0] = ports[PORT_0].at(raw);
        }
        if call.tx_phase != 0 {
            ports[PORT_6] = ports[PORT_6].at(u32::from(call.tx_phase) - 1);
        }
        if call.rx_phase != 0 {
            ports[PORT_2] = ports[PORT_2].at(u32::from(call.rx_phase) - 1);
        }
        ports[PORT_FB] = if nomul {
            Port::halves(divider)
        } else {
            Port::doubled(divider)
        };
        let feedback = ports[PORT_FB].high + ports[PORT_FB].low;
        if !ports.iter().all(|port| port.fits()) || feedback >= 64 {
            return Err(MmcmRefusal);
        }
        let iq_select = if !double_pump && lml.tx_siso_ddr {
            0b1010
        } else {
            0b1100
        };
        Ok(Self { ports, iq_select })
    }

    /// Whether every configuration a link calibration on `link` can ask is one
    /// [`Self::new`] accepts: the default phases, and any phase with each count of the RX
    /// retries' steps and each offset the TX restore adds. The phases set only the outputs'
    /// phase and delay, which fit for every phase swept.
    pub(crate) fn fits(link: &MmcmLink) -> bool {
        let steps = 0..=2;
        Self::new(link, MmcmCall::default()).is_ok()
            && steps.clone().all(|clock_reduce| {
                steps.clone().all(|phase_offset| {
                    let call = MmcmCall {
                        rx_phase: 1,
                        tx_phase: 1,
                        phase_offset,
                        clock_reduce,
                    };
                    Self::new(link, call).is_ok()
                })
            })
    }

    /// The feedback divider, which indexes `mmcm_rom`.
    fn feedback(&self) -> usize {
        let fb = self.ports[PORT_FB];
        usize::try_from(fb.high + fb.low).expect("invariant: at most 63")
    }

    /// The DRP program `mmcm_init_raw` runs for this plan, in order.
    fn program(&self) -> [DrpOp; PROGRAM] {
        let rom = MMCM_ROM[self.feedback()];
        let field = |shift: u32, bits: u32| {
            u16::try_from(rom >> shift & ((1 << bits) - 1)).expect("at most 16 bits")
        };
        let mut program = [DrpOp::Write {
            addr: drp::POWER,
            value: 0xffff,
        }; PROGRAM];
        for (index, port) in self.ports.iter().enumerate() {
            let reg = drp::CLKOUT + 2 * u8::try_from(index).expect("eight outputs");
            let six = |value: u32| u16::try_from(value & 0x3f).expect("six bits");
            let first = u16::try_from(port.phase & 7).expect("3 bits") << 13
                | six(port.high) << 6
                | six(port.low);
            // `(period_l - period_h) == 1` in signed arithmetic.
            let edge = u16::from(port.low.checked_sub(port.high) == Some(1));
            program[1 + 2 * index] = DrpOp::Update {
                addr: reg,
                keep: 1 << 12,
                set: first,
            };
            program[2 + 2 * index] = DrpOp::Update {
                addr: reg + 1,
                keep: 0xfc00,
                set: edge << 7 | six(port.delay),
            };
        }
        // `mmcm_init_raw_div(.., 1)`: odd, no-count, 0 high, 1 low.
        program[17] = DrpOp::Update {
            addr: drp::DIVCLK,
            keep: 0xc000,
            set: 1 << 13 | 1 << 12 | 1,
        };
        program[18] = DrpOp::Update {
            addr: drp::LOCK,
            keep: 0xfc00,
            set: field(20, 10),
        };
        program[19] = DrpOp::Update {
            addr: drp::LOCK + 1,
            keep: 0x8000,
            set: field(30, 5) << 10 | field(0, 10),
        };
        program[20] = DrpOp::Update {
            addr: drp::LOCK + 2,
            keep: 0x8000,
            set: field(35, 5) << 10 | field(10, 10),
        };
        // `mmcm_init_raw_filt(.., 1)`: the table's bits 59:50.
        let filter = |bit: u32, bits: u32| field(50 + bit, bits);
        program[21] = DrpOp::Update {
            addr: drp::FILTER,
            keep: 0x6000 | 0x0600 | 0x00ff,
            set: filter(9, 1) << 15 | filter(7, 2) << 11 | filter(6, 1) << 8,
        };
        program[22] = DrpOp::Update {
            addr: drp::FILTER + 1,
            keep: 0x6000 | 0x0600 | 0x0060 | 0x000f,
            set: filter(5, 1) << 15 | filter(3, 2) << 11 | filter(1, 2) << 7 | filter(0, 1) << 4,
        };
        program
    }
}

/// DRP operations `mmcm_init_raw` makes: the power write, two per output, the input
/// divider, three lock and two filter registers.
const PROGRAM: usize = 23;

/// One DRP operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrpOp {
    /// A write of `value`.
    Write {
        /// The DRP register.
        addr: u8,
        /// The value.
        value: u16,
    },
    /// A read, then a write of the bits read in `keep` with `set`.
    Update {
        /// The DRP register.
        addr: u8,
        /// The bits kept.
        keep: u16,
        /// The bits set.
        set: u16,
    },
}

/// `xsdr_override_drp`'s busy polls before it gives up waiting, and reports success.
const DRP_POLLS: u16 = 1000;
/// `xsdr_configure_lml_mmcm_tx`'s lock polls.
const LOCK_POLLS: u8 = 100;
/// The TX MMCM control: TX master (`mmcm_ctrl_sel` 4).
const TX_MASTER: u32 = 4;
/// The control's reset bit.
const MMCM_RESET: u32 = 1;
/// The status's busy flag.
const BUSY: u32 = 1 << 9;
/// The status's lock flag.
const LOCKED: u32 = 1 << 8;
/// The status's "no clock" flags.
const NO_CLOCK: u32 = 0xc00;

/// Why the TX MMCM did not come up: libusdr's `-EIO`, its "no clock" flags set or no lock
/// within its polls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MmcmUnlocked;

/// Where a configuration is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// The MMCM held in reset.
    Reset,
    /// 10 µs.
    ResetWait,
    /// DRP operation `op`: its command write (`read` for an update's read).
    Command {
        /// The operation.
        op: usize,
        /// An update's read, before its write.
        read: bool,
    },
    /// DRP operation `op`: busy poll `poll`.
    Poll {
        /// The operation.
        op: usize,
        /// An update's read.
        read: bool,
        /// The poll.
        poll: u16,
    },
    /// 10 µs between busy polls.
    PollWait {
        /// The operation.
        op: usize,
        /// An update's read.
        read: bool,
        /// The poll just made.
        poll: u16,
    },
    /// The I/Q lane selection.
    IqSelect,
    /// 10 µs.
    ReleaseWait,
    /// The reset released.
    Release,
    /// Lock poll `poll`.
    Lock(u8),
    /// 10 µs between lock polls.
    LockWait(u8),
    /// Finished.
    Done(Result<(), MmcmUnlocked>),
}

/// Writes an [`MmcmPlan`] to the TX MMCM and waits for its lock, as
/// `xsdr_configure_lml_mmcm_tx` does after computing it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConfigureMmcm {
    /// The DRP program.
    program: [DrpOp; PROGRAM],

    /// The I/Q lane selection.
    iq_select: u32,

    /// The last DRP read's data.
    read: u16,

    /// Where the configuration is.
    stage: Stage,

    /// A bus access failed.
    failed: bool,
}

impl ConfigureMmcm {
    /// Starts writing `plan`.
    pub(crate) fn new(plan: &MmcmPlan) -> Self {
        Self {
            program: plan.program(),
            iq_select: plan.iq_select,
            read: 0,
            stage: Stage::Reset,
            failed: false,
        }
    }

    /// The outcome, once finished.
    pub(crate) const fn outcome(&self) -> Option<Result<(), MmcmUnlocked>> {
        match self.stage {
            Stage::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// The DRP command operation `op` sends: a read's, or a write's of its value.
    fn command(&self, op: usize, read: bool) -> u32 {
        let (addr, value) = match self.program[op] {
            DrpOp::Write { addr, value } => (addr, Some(value)),
            DrpOp::Update { addr, .. } if read => (addr, None),
            DrpOp::Update { addr, keep, set } => (addr, Some(self.read & keep | set)),
        };
        let addr = u32::from(addr & 0x7f) << 16;
        let command = value.map_or(addr, |value| addr | 1 << 23 | u32::from(value));
        TxPhy::MmcmDrp.word(command)
    }

    /// The stage after `op`'s transaction completed.
    fn after(&self, op: usize, read: bool) -> Stage {
        if read {
            return Stage::Command { op, read: false };
        }
        match self.program.get(op + 1) {
            Some(DrpOp::Update { .. }) => Stage::Command {
                op: op + 1,
                read: true,
            },
            Some(DrpOp::Write { .. }) => Stage::Command {
                op: op + 1,
                read: false,
            },
            None => Stage::IqSelect,
        }
    }
}

impl Sequence for ConfigureMmcm {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        let write = |value| {
            SequenceAction::Fpga(FpgaAccess::Write {
                reg: REG_CFG_PHY_1,
                value,
            })
        };
        let wait = SequenceAction::Sleep(Duration::from_micros(10));
        let status = SequenceAction::Fpga(FpgaAccess::Read { reg: REG_CFG_PHY_1 });
        Some(match self.stage {
            Stage::Reset => write(TxPhy::MmcmControl.word(TX_MASTER | MMCM_RESET)),
            Stage::ReleaseWait | Stage::ResetWait | Stage::PollWait { .. } | Stage::LockWait(_) => {
                wait
            }
            Stage::Command { op, read } => write(self.command(op, read)),
            Stage::Poll { .. } | Stage::Lock(_) => status,
            Stage::IqSelect => write(TxPhy::IqSelect.word(self.iq_select)),
            Stage::Release => write(TxPhy::MmcmControl.word(TX_MASTER)),
            Stage::Done(_) => return None,
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let value = outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.stage = match self.stage {
            Stage::Reset => Stage::ResetWait,
            Stage::ResetWait => match self.program[0] {
                DrpOp::Update { .. } => Stage::Command { op: 0, read: true },
                DrpOp::Write { .. } => Stage::Command { op: 0, read: false },
            },
            Stage::Command { op, read } => Stage::Poll { op, read, poll: 0 },
            Stage::Poll { op, read, poll } => {
                // libusdr takes the last status, even one still busy after every poll.
                if read {
                    self.read = u16::try_from(value >> 16).expect("16 bits");
                }
                if value & BUSY == 0 {
                    self.after(op, read)
                } else {
                    Stage::PollWait { op, read, poll }
                }
            }
            // The wait after the last busy poll, then on without an error, as libusdr.
            Stage::PollWait { op, read, poll } if poll + 1 >= DRP_POLLS => self.after(op, read),
            Stage::PollWait { op, read, poll } => Stage::Poll {
                op,
                read,
                poll: poll + 1,
            },
            Stage::IqSelect => Stage::ReleaseWait,
            Stage::ReleaseWait => Stage::Release,
            Stage::Release => Stage::Lock(0),
            Stage::Lock(poll) => {
                if value & NO_CLOCK != 0 {
                    Stage::Done(Err(MmcmUnlocked))
                } else if value & LOCKED != 0 {
                    Stage::Done(Ok(()))
                } else {
                    Stage::LockWait(poll)
                }
            }
            Stage::LockWait(poll) if poll + 1 >= LOCK_POLLS => Stage::Done(Err(MmcmUnlocked)),
            Stage::LockWait(poll) => Stage::Lock(poll + 1),
            Stage::Done(outcome) => Stage::Done(outcome),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The link a `/dm/rate/master` of `rate` leaves, MIMO, not double-pumped: CGEN, the
    /// DAC's and processor's dividers, and the `LimeLight` TX divider.
    fn link(cgen: u32, dac: u32, tsp: u32, lml_div: u8) -> MmcmLink {
        MmcmLink {
            clocks: Clocks {
                cgen,
                tx_converter_div: dac,
                tx_tsp_div: tsp,
                ..Clocks::default()
            },
            lml: LmlConfig {
                tx_div: lml_div,
                ..LmlConfig::default()
            },
            double_pump: false,
            separate_clkdiv: false,
        }
    }

    // Expected values: libusdr's arithmetic worked by hand.

    #[test]
    fn a_20_mhz_link_clock_caps_the_vco_divider_at_63() {
        // CGEN 320 MHz, DAC / 4, LML / 4: MCLK 20 MHz; the processor interpolates, so no
        // multiplication. ceil(1600 / 20) = 80, capped at 63; 63 * 20 MHz reaches 800 MHz.
        let plan =
            MmcmPlan::new(&link(320_000_000, 4, 8, 4), MmcmCall::default()).expect("configurable");
        assert_eq!(plan.ports[PORT_FB], Port::halves(63), "nomul: feedback 63");
        assert_eq!(plan.feedback(), 63);
        // TX fabric clock at 90 degrees: 4 * 63 eighths.
        assert_eq!(
            (plan.ports[PORT_6].phase, plan.ports[PORT_6].delay),
            (4, 31)
        );
        assert_eq!(plan.iq_select, 0b1100);
    }

    #[test]
    fn a_clock_too_slow_for_the_vco_doubles_the_feedback_past_the_table() {
        // 1 MS/s: MCLK 2 MHz. 63 * 2 MHz is below 800 MHz, so libusdr takes
        // ceil(1600 / 2) = 800, capped at 126: feedback 126, past the 64 entries.
        assert_eq!(
            MmcmPlan::new(&link(128_000_000, 4, 32, 16), MmcmCall::default()),
            Err(MmcmRefusal)
        );
        // 12 698 413 Hz is the slowest clock 63 multiplies to 800 MHz.
        let slowest = link(12_698_413, 1, 2, 1);
        assert!(MmcmPlan::new(&slowest, MmcmCall::default()).is_ok());
        let slower = link(12_698_412, 1, 2, 1);
        assert_eq!(
            MmcmPlan::new(&slower, MmcmCall::default()),
            Err(MmcmRefusal)
        );
    }

    #[test]
    fn a_link_fits_only_if_every_retry_and_offset_does() {
        assert!(MmcmPlan::fits(&link(320_000_000, 4, 8, 4)));
        assert!(!MmcmPlan::fits(&link(128_000_000, 4, 32, 16)), "1 MS/s");
        // Double-pumped at 40 MHz MCLK: ceil(1600 / 80) = 20, doubled feedback 40; four
        // more steps give 48, still in the table. At 25 MHz: 32 doubled is 64, past it.
        let pumped = |mclk: u32| MmcmLink {
            double_pump: true,
            ..link(mclk, 1, 1, 1)
        };
        assert!(MmcmPlan::fits(&pumped(40_000_000)));
        assert!(!MmcmPlan::fits(&pumped(25_000_000)));
    }

    #[test]
    fn phases_set_their_outputs_and_retries_widen_the_divider() {
        // 40 MHz MCLK, mul: io 80 MHz, ceil(1600 / 80) = 20; two retries and an offset of
        // 5 (counted as 2) add 4.
        let link = MmcmLink {
            double_pump: true,
            ..link(160_000_000, 1, 1, 4)
        };
        let call = MmcmCall {
            rx_phase: 10,
            tx_phase: 3,
            phase_offset: 5,
            clock_reduce: 2,
        };
        let plan = MmcmPlan::new(&link, call).expect("configurable");
        assert_eq!(plan.ports[PORT_FB], Port::doubled(24), "mul: feedback 48");
        assert_eq!((plan.ports[PORT_2].phase, plan.ports[PORT_2].delay), (1, 1));
        assert_eq!((plan.ports[PORT_6].phase, plan.ports[PORT_6].delay), (2, 0));
        assert_eq!(plan.ports[PORT_5], Port::doubled(24), "double-pumped");
    }

    /// The writes a configuration made, the first `len`; fixed-size, with no allocation.
    struct Writes {
        /// The writes.
        values: [u32; 64],
        /// Writes made.
        len: usize,
    }

    impl Writes {
        /// The writes made.
        fn made(&self) -> &[u32] {
            &self.values[..self.len]
        }
    }

    /// Runs a configuration on a TX PHY whose status reads `status` after the last write,
    /// returning the writes it made and its outcome.
    fn run(plan: &MmcmPlan, status: impl Fn(u32) -> u32) -> (Writes, Result<(), MmcmUnlocked>) {
        let mut configure = ConfigureMmcm::new(plan);
        let mut writes = Writes {
            values: [0; 64],
            len: 0,
        };
        while let Some(action) = configure.next() {
            let value = match action {
                SequenceAction::Fpga(FpgaAccess::Write { value, .. }) => {
                    writes.values[writes.len] = value;
                    writes.len += 1;
                    0
                }
                SequenceAction::Fpga(FpgaAccess::Read { .. }) => {
                    status(writes.made().last().copied().expect("a command first"))
                }
                _ => 0,
            };
            configure.complete::<()>(Ok(value)).expect("no failure");
        }
        (writes, configure.outcome().expect("finished"))
    }

    #[test]
    fn an_update_keeps_the_bits_read_and_the_lock_ends_the_configuration() {
        let plan =
            MmcmPlan::new(&link(320_000_000, 4, 8, 4), MmcmCall::default()).expect("configurable");
        // Every DRP read returns all ones; the MMCM locks once released.
        let (writes, outcome) = run(
            &plan,
            |last| {
                if last >> 24 == 1 { 0xffff_0000 } else { LOCKED }
            },
        );
        assert_eq!(outcome, Ok(()));
        let writes = writes.made();
        assert_eq!(writes[0], 2 << 24 | 5, "TX master held in reset");
        assert_eq!(
            writes[1],
            1 << 24 | 1 << 23 | 0x28 << 16 | 0xffff,
            "power on"
        );
        // Output 0's first register: read, then phase 0, high 31, low 32 over bit 12.
        assert_eq!(writes[2], 1 << 24 | 0x06 << 16);
        assert_eq!(
            writes[3],
            1 << 24 | 1 << 23 | 0x06 << 16 | 1 << 12 | 31 << 6 | 32
        );
        assert_eq!(writes[writes.len() - 2], 5 << 24 | 0b1100, "I/Q lanes");
        assert_eq!(writes[writes.len() - 1], 2 << 24 | 4, "released");
        // 23 operations: one write and 22 updates of two commands each.
        assert_eq!(writes.len(), 1 + 45 + 2);
    }

    #[test]
    fn no_clock_or_no_lock_is_libusdrs_eio() {
        let plan =
            MmcmPlan::new(&link(320_000_000, 4, 8, 4), MmcmCall::default()).expect("configurable");
        let (_, no_clock) = run(&plan, |_| NO_CLOCK);
        assert_eq!(no_clock, Err(MmcmUnlocked));
        let (_, unlocked) = run(&plan, |_| 0);
        assert_eq!(unlocked, Err(MmcmUnlocked));
    }
}
