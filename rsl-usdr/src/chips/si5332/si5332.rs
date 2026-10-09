//! The Si5332 driver: identification, the power-up output plan and the sample-clock plan.

use std::time::Duration;

use bnb::{u2, u5, u6};

use super::divider::{DividerPowerDown, HsBank, HsDivider, SpreadBank, SpreadSpectrum};
use super::input::{
    CrystalLoad, CrystalPin, CrystalTrim, InputMode, InputPowerDown, PllReference, Prescaler,
};
use super::layout::Layout;
use super::output::{
    CmosDrive, DirectSource, Divider, DividerSource, DriverMode, Output, Output0to3PowerDown,
    Output4and5PowerDown, OutputSource, Polarity, Skew, SourcePowerDown,
};
use super::reg::{ID_REGS, Reg};
use super::state::{CurrentState, RequestedState};
use crate::chips::register::{I2cChip, IndexedRegister, Register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Polls of [`CurrentState`] before giving up, 10 µs apart (`si5532_get_state`).
const STATE_POLLS: usize = 100;

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

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// Where the chip answers.
    addr: I2cAddr,
}

impl I2cChip for Si5332 {
    const NAME: &'static str = "Si5332";

    fn addr(&self) -> I2cAddr {
        self.addr
    }
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { addr: dev }
    }

    /// The clock generator where the uSDR wires it: FPGA I2C bus 0, address 0x6A
    /// (`I2C_DEV_CLKGEN` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x6a))
    }

    /// Identifies the chip and writes its power-up output plan, with output 0 divided by
    /// `div`: `si5332_init` up to its final state poll, which [`Self::wait_active`] does.
    pub(crate) fn program(
        self,
        bus: &mut dyn Bus,
        div: u8,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Result<(), Error> {
        let mut regs = self.on(bus);
        let mut id = [0; ID_REGS.len()];
        for (reg, byte) in ID_REGS.into_iter().zip(&mut id) {
            *byte = regs.read_raw(reg)?;
        }
        regs.read_raw(Reg::SupplyStatus)?;
        if id[..6].iter().all(|&byte| byte == 0xff) {
            return Err(Error::ChipMissing(Self::NAME));
        }

        // Like libusdr, a state other than ACTIVE is not an error before programming.
        self.settle(bus)?;
        let mut regs = self.on(bus);
        for (reg, value) in Self::power_up_plan(div, reference, lvpecl) {
            regs.write_raw(reg, value)?;
        }
        Ok(())
    }

    /// Waits for the state requested last to settle; a missing input clock is an error.
    ///
    /// Like libusdr, a state that never settles is not.
    pub(crate) fn wait_active(self, bus: &mut dyn Bus) -> Result<(), Error> {
        match self.settle(bus)? {
            CurrentState::NoInputClock => Err(Error::ClockInputMissing),
            _ => Ok(()),
        }
    }

    /// Moves the RX and TX sample clocks onto the plan in `layout`, usually the PLL (the
    /// reference itself when [`Layout`] says so), and points output 3,
    /// the mixer LO, at the VCO divided by `lo_divider` (`si5332_set_layout`). The RX clock is
    /// whichever of outputs 0 and 1 is not `lvpecl`.
    ///
    /// The writes run in a READY..ACTIVE cycle; like libusdr, a state other than ACTIVE is
    /// not an error before it, and a missing input clock is one after it.
    pub(crate) fn set_layout(
        self,
        bus: &mut dyn Bus,
        layout: &Layout,
        lvpecl: LvpeclOutput,
        lo_divider: u8,
    ) -> Result<(), Error> {
        self.settle(bus)?;
        let mut regs = self.on(bus);
        for (reg, value) in Self::layout_plan(layout, lvpecl, lo_divider) {
            regs.write_raw(reg, value)?;
        }
        self.wait_active(bus)
    }

    /// The register writes of `si5332_set_layout`, in order.
    fn layout_plan(layout: &Layout, lvpecl: LvpeclOutput, lo_divider: u8) -> [RegWrite; 25] {
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
    fn power_up_plan(div: u8, reference: Reference, lvpecl: LvpeclOutput) -> [RegWrite; 49] {
        let (pll_reference, input2, inputs_off) = match reference {
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
        };
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
            (Reg::OutputEnables0to3, 0xff),
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

    /// Polls [`CurrentState`] until it leaves the transitional values, or gives up after
    /// [`STATE_POLLS`], and returns the last state read (`si5532_get_state`).
    fn settle(self, bus: &mut dyn Bus) -> Result<CurrentState, Error> {
        let mut state = CurrentState::Other(0);
        for _ in 0..STATE_POLLS {
            bus.sleep(Duration::from_micros(10));
            state = self.on(bus).read()?;
            if !matches!(state, CurrentState::Other(_)) {
                break;
            }
        }
        Ok(state)
    }
}

/// The low byte, as libusdr's `uint8_t` register table stores it.
fn low_byte(value: u32) -> u8 {
    value.to_le_bytes()[0]
}
