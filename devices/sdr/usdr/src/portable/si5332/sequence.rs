//! The Si5332's register sequences without I/O: `si5332_init`'s identification and power-up
//! plan, its state wait, and `si5332_set_layout`'s sample-clock plan.
//!
//! [`ClockSequence`] is a [`Sequence`]: register reads and writes, plus the 10 µs waits of
//! libusdr's state poll (`si5532_get_state`).

use core::time::Duration;

use bnb::{u2, u5, u6};

use super::super::address::I2cAddr;
use super::super::clock::Layout;
use super::super::register::{IndexedRegister, Register};
use super::super::sequence::{RegisterAccess, Sequence, SequenceAction, SequenceFailure, i2c_byte};
use super::NAME;
use super::divider::{DividerPowerDown, HsBank, HsDivider, SpreadBank, SpreadSpectrum};
use super::input::{
    CrystalLoad, CrystalPin, CrystalTrim, InputMode, InputPowerDown, PllReference, Prescaler,
};
use super::output::{
    CmosDrive, DirectSource, Divider, DividerSource, DriverMode, Output, Output0to3PowerDown,
    Output4and5PowerDown, OutputEnables0to3, OutputSource, Polarity, Skew, SourcePowerDown,
};
use super::reg::{ID_REGS, Reg};
use super::state::{CurrentState, RequestedState};

/// Polls of [`CurrentState`] before giving up (`si5532_get_state`).
const STATE_POLLS: u8 = 100;

/// The wait before each state poll.
const POLL_INTERVAL: Duration = Duration::from_micros(10);

/// Writes in the power-up plan.
pub(crate) const POWER_UP_WRITES: usize = 49;

/// Writes in the sample-clock plan.
pub(crate) const LAYOUT_WRITES: usize = 25;

/// Writes in the reference switch.
pub(crate) const SWITCH_WRITES: usize = 5;

/// Where the reference comes from: the clock every output is derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// The on-board 26 MHz oscillator, which revision 3 carries and the FPGA switches on.
    Oscillator,

    /// Clock input 2, received differentially: the reference on revisions 1 and 2.
    Input2,
}

/// Which of outputs 0 and 1 drives LVPECL; the other drives CMOS.
///
/// The LVPECL output carries the LMS6002D's PLL reference, and the CMOS one the RX sample
/// clock. Revision 3 swapped the two: output 0 is the PLL reference there, output 1 on
/// revisions 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LvpeclOutput {
    /// Output 0 is LVPECL.
    Out0,

    /// Output 1 is LVPECL.
    Out1,
}

/// One register write: address and byte.
type RegWrite = (Reg, u8);

/// What a finished state wait leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterSettle {
    /// The plan's writes; any settled state is accepted first, as in libusdr.
    Writes,
    /// The end; a missing input clock is an error, a state that never settles is not.
    Check,
}

/// Where the sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Reading identification register `ID_REGS[index]`.
    Identify(usize),
    /// Reading the supply status, which libusdr reads and ignores.
    Supply,
    /// Waiting before a state poll, with `left` polls remaining.
    Wait {
        /// Polls remaining, this one included.
        left: u8,
        /// What the settled state leads to.
        then: AfterSettle,
    },
    /// Reading the state, with `left` polls remaining.
    Poll {
        /// Polls remaining, this one included.
        left: u8,
        /// What the settled state leads to.
        then: AfterSettle,
    },
    /// Writing plan entry `index`.
    Write(usize),
    /// Finished.
    Done,
    /// A step failed; nothing more is issued.
    Failed,
}

/// One of the Si5332 sequences, with a plan of `N` writes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClockSequence<const N: usize> {
    /// Where the chip answers.
    dev: I2cAddr,
    /// The writes, in order.
    plan: [RegWrite; N],
    /// After the writes, wait for the state and require an input clock.
    check_after: bool,
    /// The identification bytes read so far.
    id: [u8; ID_REGS.len()],
    /// The action [`Sequence::next`] returns.
    phase: Phase,
}

impl ClockSequence<POWER_UP_WRITES> {
    /// Identifies the chip and writes its power-up output plan, with output 0 divided by
    /// `div`: `si5332_init` up to its final state poll, which [`ClockSequence::wait_active`]
    /// does. A chip whose identification reads all 0xFF is missing.
    pub(crate) fn program(
        dev: I2cAddr,
        div: u8,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Self {
        Self {
            dev,
            plan: power_up_plan(div, reference, lvpecl),
            check_after: false,
            id: [0; ID_REGS.len()],
            phase: Phase::Identify(0),
        }
    }
}

impl ClockSequence<0> {
    /// Waits for the state requested last to settle; a missing input clock is an error.
    pub(crate) fn wait_active(dev: I2cAddr) -> Self {
        Self {
            dev,
            plan: [],
            check_after: true,
            id: [0; ID_REGS.len()],
            phase: Phase::Wait {
                left: STATE_POLLS,
                then: AfterSettle::Check,
            },
        }
    }
}

impl ClockSequence<LAYOUT_WRITES> {
    /// Moves the RX and TX sample clocks onto the plan in `layout`, and points output 3,
    /// the mixer LO, at the VCO divided by `lo_divider` (`si5332_set_layout`). The RX clock
    /// is whichever of outputs 0 and 1 is not `lvpecl`. The writes run in a READY..ACTIVE
    /// cycle; like libusdr, a state other than ACTIVE is not an error before it, and a
    /// missing input clock is one after it.
    pub(crate) fn layout(
        dev: I2cAddr,
        layout: &Layout,
        lvpecl: LvpeclOutput,
        lo_divider: u8,
    ) -> Self {
        Self {
            dev,
            plan: layout_plan(layout, lvpecl, lo_divider),
            check_after: true,
            id: [0; ID_REGS.len()],
            phase: Phase::Wait {
                left: STATE_POLLS,
                then: AfterSettle::Writes,
            },
        }
    }
}

impl ClockSequence<SWITCH_WRITES> {
    /// Switches the PLL's reference to `reference` in a READY..ACTIVE cycle
    /// (`si5532_set_ext_clock_sw`). Like libusdr, a state other than ACTIVE is not an error
    /// before it, and a missing input clock is one after it.
    pub(crate) fn reference_switch(dev: I2cAddr, reference: Reference) -> Self {
        let (pll_reference, input2, inputs_off) = reference_inputs(reference);
        Self {
            dev,
            plan: [
                RequestedState::Ready.entry(),
                pll_reference.entry(),
                input2.entry(),
                inputs_off.entry(),
                RequestedState::Active.entry(),
            ],
            check_after: true,
            id: [0; ID_REGS.len()],
            phase: Phase::Wait {
                left: STATE_POLLS,
                then: AfterSettle::Writes,
            },
        }
    }
}

/// The PLL reference, clock input 2's mode and the input power-downs `reference` needs.
fn reference_inputs(reference: Reference) -> (PllReference, InputMode, InputPowerDown) {
    match reference {
        Reference::Oscillator => (
            PllReference::Oscillator,
            InputMode::Off,
            InputPowerDown::new().with_input_buffer0(true),
        ),
        Reference::Input2 => (
            PllReference::Input2,
            InputMode::Differential,
            InputPowerDown::new().with_oscillator(true),
        ),
    }
}

impl<const N: usize> ClockSequence<N> {
    /// The phase after the plan's writes (or in place of them, when there are none).
    fn after_writes(&self) -> Phase {
        if self.check_after {
            Phase::Wait {
                left: STATE_POLLS,
                then: AfterSettle::Check,
            }
        } else {
            Phase::Done
        }
    }

    /// The first write, or what follows the writes when the plan is empty.
    fn first_write(&self) -> Phase {
        if N == 0 {
            self.after_writes()
        } else {
            Phase::Write(0)
        }
    }

    /// Ends the sequence with `failure`.
    fn fail<E>(&mut self, failure: SequenceFailure<E>) -> Result<(), SequenceFailure<E>> {
        self.phase = Phase::Failed;
        Err(failure)
    }
}

impl<const N: usize> Sequence for ClockSequence<N> {
    fn next(&self) -> Option<SequenceAction> {
        let read = |reg: Reg| SequenceAction::Access(RegisterAccess::read(self.dev, reg));
        Some(match self.phase {
            Phase::Identify(index) => read(ID_REGS[index]),
            Phase::Supply => read(Reg::SupplyStatus),
            Phase::Wait { .. } => SequenceAction::Sleep(POLL_INTERVAL),
            Phase::Poll { .. } => read(Reg::CurrentState),
            Phase::Write(index) => {
                SequenceAction::Access(RegisterAccess::write(self.dev, self.plan[index]))
            }
            Phase::Done | Phase::Failed => return None,
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let byte = match (i2c_byte(result), action) {
            (Ok(byte), _) => byte,
            (Err(source), SequenceAction::Access(access)) => {
                return self.fail(SequenceFailure::Transport {
                    chip: NAME,
                    access,
                    source,
                });
            }
            (Err(source), SequenceAction::Fpga(access)) => {
                return self.fail(SequenceFailure::FpgaTransport {
                    chip: NAME,
                    access,
                    source,
                });
            }
            // A wait cannot fail; its outcome only says it has passed.
            (Err(_), SequenceAction::Sleep(_)) => 0,
        };
        self.phase = match self.phase {
            Phase::Identify(index) => {
                self.id[index] = byte;
                if index + 1 < ID_REGS.len() {
                    Phase::Identify(index + 1)
                } else {
                    Phase::Supply
                }
            }
            Phase::Supply => {
                if self.id[..6].iter().all(|&id| id == 0xff) {
                    return self.fail(SequenceFailure::ChipMissing { chip: NAME });
                }
                // Like libusdr, a state other than ACTIVE is not an error before programming.
                Phase::Wait {
                    left: STATE_POLLS,
                    then: AfterSettle::Writes,
                }
            }
            Phase::Wait { left, then } => Phase::Poll { left, then },
            Phase::Poll { left, then } => {
                let state = CurrentState::from(byte);
                if matches!(state, CurrentState::Other(_)) && left > 1 {
                    Phase::Wait {
                        left: left - 1,
                        then,
                    }
                } else {
                    match then {
                        AfterSettle::Writes => self.first_write(),
                        AfterSettle::Check if state == CurrentState::NoInputClock => {
                            return self.fail(SequenceFailure::InputMissing { chip: NAME });
                        }
                        AfterSettle::Check => Phase::Done,
                    }
                }
            }
            Phase::Write(index) if index + 1 < N => Phase::Write(index + 1),
            Phase::Write(_) => self.after_writes(),
            // `next` returned an action, so neither is current.
            Phase::Done | Phase::Failed => self.phase,
        };
        Ok(())
    }
}

/// The register writes of `si5332_set_layout`, in order.
fn layout_plan(layout: &Layout, lvpecl: LvpeclOutput, lo_divider: u8) -> [RegWrite; LAYOUT_WRITES] {
    let rx_clock = match lvpecl {
        LvpeclOutput::Out0 => Output::Out1,
        LvpeclOutput::Out1 => Output::Out0,
    };
    // Slew 0 is fastest, 3 slowest.
    let slew = match layout.out_hz {
        110_000_001.. => 0,
        50_000_001.. => 1,
        25_000_001.. => 2,
        _ => 3,
    };
    let drive = CmosDrive::new().with_slew(u2::new(slew));
    let divider = Divider::new().with_ratio(u6::new(low_byte(layout.output_divider)));
    let source = OutputSource::new()
        .with_direct(DirectSource::PllReference)
        .with_divider(if layout.from_reference {
            DividerSource::Direct
        } else {
            DividerSource::HighSpeed0
        });
    let pll_mode = if layout.pll_input_hz > 30_000_000 {
        8
    } else {
        4
    };
    // libusdr writes each 15-bit term's low byte to the higher address; see `Reg`.
    let [integer_low, integer_high, ..] = layout.integer.to_le_bytes();
    let [residue_low, residue_high, ..] = layout.residue.to_le_bytes();
    let [denominator_low, denominator_high, ..] = layout.denominator.to_le_bytes();
    let hs_divider = HsDivider::new().with_ratio(low_byte(layout.hs_divider));
    let lo_divider = HsDivider::new().with_ratio(lo_divider);
    let prescaler = Prescaler::new().with_ratio(u5::new(low_byte(layout.prescaler)));

    #[rustfmt::skip]
    let plan = [
        RequestedState::Ready.entry(),
        (Reg::PllInteger68, integer_low),
        (Reg::PllInteger67, integer_high),
        (Reg::PllResidue6A, residue_low),
        (Reg::PllResidue69, residue_high),
        (Reg::PllDenominator6C, denominator_low),
        (Reg::PllDenominator6B, denominator_high),
        prescaler.entry(),
        (Reg::PllMode, pll_mode),
        hs_divider.entry_at(HsBank::Div0A),
        hs_divider.entry_at(HsBank::Div0B),
        HsDivider::new().entry_at(HsBank::Div1A),
        HsDivider::new().entry_at(HsBank::Div2A),
        drive.entry_at(rx_clock),
        drive.entry_at(Output::Out2),
        divider.entry_at(rx_clock),
        divider.entry_at(Output::Out2),
        source.entry_at(rx_clock),
        source.entry_at(Output::Out2),
        lo_divider.entry_at(HsBank::Div3A),
        lo_divider.entry_at(HsBank::Div3B),
        OutputSource::new()
            .with_direct(DirectSource::PllReference)
            .with_divider(DividerSource::HighSpeed3)
            .entry_at(Output::Out3),
        Divider::new().with_ratio(u6::new(1)).entry_at(Output::Out3),
        DriverMode::Hcsl50Internal.entry_at(Output::Out3),
        RequestedState::Active.entry(),
    ];
    plan
}

/// The register writes of `si5332_init`, in order: hold the outputs, route every output
/// straight from the reference, set each output's driver, power down unused blocks, run.
fn power_up_plan(
    div: u8,
    reference: Reference,
    lvpecl: LvpeclOutput,
) -> [RegWrite; POWER_UP_WRITES] {
    let (pll_reference, input2, inputs_off) = reference_inputs(reference);
    let (out0_mode, out1_mode) = match lvpecl {
        LvpeclOutput::Out0 => (DriverMode::Lvpecl, DriverMode::CmosPositive),
        LvpeclOutput::Out1 => (DriverMode::CmosPositive, DriverMode::Lvpecl),
    };
    let reference_direct = OutputSource::new()
        .with_divider(DividerSource::Direct)
        .with_direct(DirectSource::PllReference);
    let divide_by = |ratio| Divider::new().with_ratio(u6::new(ratio));
    let slow_cmos = CmosDrive::new().with_slew(u2::new(3));
    let no_spread = SpreadSpectrum::new();
    let dividers_off = DividerPowerDown::new()
        .with_high_speed1(true)
        .with_high_speed2(true)
        .with_high_speed4(true)
        .with_interpolative0(true)
        .with_interpolative1(true);

    #[rustfmt::skip]
    let plan = [
        RequestedState::Ready.entry(),
        pll_reference.entry(),
        input2.entry(),
        (Reg::Input3Mode, 0),
        no_spread.entry_at(SpreadBank::Id0a),
        no_spread.entry_at(SpreadBank::Id0b),
        no_spread.entry_at(SpreadBank::Id1a),
        no_spread.entry_at(SpreadBank::Id1b),
        reference_direct.entry_at(Output::Out0),
        reference_direct.entry_at(Output::Out1),
        reference_direct.entry_at(Output::Out2),
        reference_direct.entry_at(Output::Out3),
        reference_direct.entry_at(Output::Out4),
        reference_direct.entry_at(Output::Out5),
        out0_mode.entry_at(Output::Out0),
        Divider::new().with_ratio(u6::new(div)).entry_at(Output::Out0),
        Skew::new().entry_at(Output::Out0),
        Polarity::new().entry_at(Output::Out0),
        slow_cmos.entry_at(Output::Out0),
        out1_mode.entry_at(Output::Out1),
        divide_by(1).entry_at(Output::Out1),
        Skew::new().entry_at(Output::Out1),
        Polarity::new().entry_at(Output::Out1),
        slow_cmos.entry_at(Output::Out1),
        DriverMode::CmosPositive.entry_at(Output::Out2),
        divide_by(1).entry_at(Output::Out2),
        Skew::new().entry_at(Output::Out2),
        Polarity::new().entry_at(Output::Out2),
        slow_cmos.entry_at(Output::Out2),
        divide_by(0).entry_at(Output::Out3),
        divide_by(0).entry_at(Output::Out4),
        DriverMode::Off.entry_at(Output::Out5),
        divide_by(1).entry_at(Output::Out5),
        Skew::new().entry_at(Output::Out5),
        Polarity::new().entry_at(Output::Out5),
        slow_cmos.entry_at(Output::Out5),
        DriverMode::Off.entry_at(Output::Out3),
        DriverMode::Off.entry_at(Output::Out4),
        OutputEnables0to3::ALL.entry(),
        (Reg::OutputEnables4and5, 0xff),
        inputs_off.entry(),
        dividers_off.entry(),
        SourcePowerDown::new().with_output4(true).with_output5(true).entry(),
        Output0to3PowerDown::new().entry(),
        Output4and5PowerDown::new().with_output4(true).with_output5(true).entry(),
        CrystalLoad::new().entry(),
        CrystalTrim::new().entry_at(CrystalPin::Xa),
        CrystalTrim::new().entry_at(CrystalPin::Xb),
        RequestedState::Active.entry(),
    ];
    plan
}

/// The low byte, as libusdr's `uint8_t` register table stores it.
fn low_byte(value: u32) -> u8 {
    value.to_le_bytes()[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the uSDR's Si5332 answers.
    const DEV: I2cAddr = I2cAddr::new(0, 0x6a);

    /// `USYS_STAT` values.
    const ACTIVE: u8 = 0x02;
    const NO_INPUT_CLOCK: u8 = 0x89;
    const TRANSITIONING: u8 = 0x00;

    /// What a run did: its reads of `USYS_STAT`, waits and writes.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Counts {
        polls: usize,
        waits: usize,
        writes: usize,
    }

    /// Runs `sequence` to its end. Identification reads return `id`; state polls return
    /// `states` in order, then the last one forever.
    fn run<const N: usize>(
        sequence: &mut ClockSequence<N>,
        id: u8,
        states: &[u8],
    ) -> (Counts, Result<(), SequenceFailure<()>>) {
        let mut counts = Counts::default();
        while let Some(action) = sequence.next() {
            let byte = match action {
                SequenceAction::Sleep(wait) => {
                    assert_eq!(wait, POLL_INTERVAL);
                    counts.waits += 1;
                    0
                }
                SequenceAction::Access(RegisterAccess::Read { reg, .. })
                    if reg == u8::from(Reg::CurrentState) =>
                {
                    counts.polls += 1;
                    states[(counts.polls - 1).min(states.len() - 1)]
                }
                SequenceAction::Access(RegisterAccess::Read { .. }) => id,
                SequenceAction::Access(RegisterAccess::Write { .. }) => {
                    counts.writes += 1;
                    0
                }
                SequenceAction::Fpga(access) => {
                    panic!("no Si5332 step touches the FPGA: {access:?}")
                }
            };
            if let Err(failure) = sequence.complete::<()>(Ok(byte.into())) {
                return (counts, Err(failure));
            }
        }
        (counts, Ok(()))
    }

    #[test]
    fn a_transitional_state_is_polled_again_after_another_wait() {
        let mut wait = ClockSequence::wait_active(DEV);
        let (counts, outcome) = run(&mut wait, 0, &[TRANSITIONING, TRANSITIONING, ACTIVE]);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            counts,
            Counts {
                polls: 3,
                waits: 3,
                writes: 0
            },
            "each poll follows its own 10 µs wait"
        );
    }

    #[test]
    fn a_state_that_never_settles_is_accepted_after_a_hundred_polls() {
        // si5532_get_state returns the last state read; si5332_init then checks only for
        // a missing input clock.
        let mut wait = ClockSequence::wait_active(DEV);
        let (counts, outcome) = run(&mut wait, 0, &[TRANSITIONING]);
        assert_eq!(outcome, Ok(()));
        assert_eq!(counts.polls, usize::from(STATE_POLLS));
    }

    #[test]
    fn a_missing_input_clock_fails_only_the_wait_after_the_writes() {
        let layout = Layout::new(26_000_000, 20_000_000).expect("a valid sample clock");
        let mut sequence = ClockSequence::layout(DEV, &layout, LvpeclOutput::Out0, 8);
        let (counts, outcome) = run(&mut sequence, 0, &[NO_INPUT_CLOCK]);
        assert_eq!(outcome, Err(SequenceFailure::InputMissing { chip: NAME }));
        assert_eq!(
            counts.writes, LAYOUT_WRITES,
            "the state before the plan is not checked"
        );
        assert_eq!(sequence.next(), None);
    }

    #[test]
    fn an_absent_chip_is_missing_before_anything_is_written() {
        let mut program = ClockSequence::program(DEV, 1, Reference::Oscillator, LvpeclOutput::Out0);
        let (counts, outcome) = run(&mut program, 0xff, &[ACTIVE]);
        assert_eq!(outcome, Err(SequenceFailure::ChipMissing { chip: NAME }));
        assert_eq!(counts, Counts::default(), "only identification was read");
    }

    #[test]
    fn programming_waits_for_any_settled_state_then_writes_the_whole_plan() {
        let mut program = ClockSequence::program(DEV, 1, Reference::Input2, LvpeclOutput::Out1);
        let (counts, outcome) = run(&mut program, 0x00, &[NO_INPUT_CLOCK]);
        assert_eq!(
            outcome,
            Ok(()),
            "no input clock is not an error before programming"
        );
        assert_eq!(counts.writes, POWER_UP_WRITES);
        assert_eq!(counts.polls, 1);
    }

    #[test]
    fn a_failed_poll_names_the_state_register() {
        let mut wait = ClockSequence::wait_active(DEV);
        wait.complete::<()>(Ok(0)).unwrap();
        assert_eq!(
            wait.complete(Err("bus down")),
            Err(SequenceFailure::Transport {
                chip: NAME,
                access: RegisterAccess::Read {
                    dev: DEV,
                    reg: u8::from(Reg::CurrentState),
                },
                source: "bus down",
            })
        );
        assert_eq!(wait.next(), None);
    }
}
