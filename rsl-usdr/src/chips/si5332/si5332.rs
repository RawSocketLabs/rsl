//! The Si5332 driver: identification and the power-up output plan.

use std::time::Duration;

use bnb::{u2, u6};

use super::divider::{DividerPowerDown, SpreadBank, SpreadSpectrum};
use super::input::{CrystalLoad, CrystalPin, CrystalTrim, InputMode, InputPowerDown, PllReference};
use super::output::{
    CmosDrive, DirectSource, Divider, DividerSource, DriverMode, Output, Output0to3PowerDown,
    Output4and5PowerDown, OutputSource, Polarity, Skew, SourcePowerDown,
};
use super::reg::{ID_REGS, Reg};
use super::state::{CurrentState, RequestedState};
use crate::chips::register::{I2cRegisters, IndexedRegister, Register};
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
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "Si5332 read", "Si5332 write"),
        }
    }

    /// The clock generator where the uSDR wires it: FPGA I2C bus 0, address 0x6A
    /// (`I2C_DEV_CLKGEN` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x6a))
    }

    /// Identifies the chip and programs its power-up output plan (`si5332_init`), with
    /// output 0 divided by `div`.
    pub(crate) fn init(
        self,
        bus: &mut dyn Bus,
        div: u8,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Result<(), Error> {
        let mut regs = self.regs.on(bus);
        let mut id = [0; ID_REGS.len()];
        for (reg, byte) in ID_REGS.into_iter().zip(&mut id) {
            *byte = regs.read_raw(reg)?;
        }
        regs.read_raw(Reg::SupplyStatus)?;
        if id[..6].iter().all(|&byte| byte == 0xff) {
            return Err(Error::ChipMissing("Si5332"));
        }

        self.wait_state(bus, false)?;
        let mut regs = self.regs.on(bus);
        for (reg, value) in Self::power_up_plan(div, reference, lvpecl) {
            regs.write_raw(reg, value)?;
        }
        // libusdr reads FPGA register 0xC here and ignores the result; kept so traces line up.
        let _ = bus.read_regs(0xc, &mut [0]);
        self.wait_state(bus, true)
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

    /// Polls [`CurrentState`] until it settles (`si5532_get_state`).
    ///
    /// Like libusdr, a state that never settles is not an error, and a missing input clock
    /// is one only once programming is done (`after_init`).
    fn wait_state(self, bus: &mut dyn Bus, after_init: bool) -> Result<(), Error> {
        let mut state = CurrentState::Other(0);
        for _ in 0..STATE_POLLS {
            bus.sleep(Duration::from_micros(10));
            state = self.regs.on(bus).read()?;
            if !matches!(state, CurrentState::Other(_)) {
                break;
            }
        }
        match state {
            CurrentState::NoInputClock if after_init => Err(Error::ClockInputMissing),
            _ => Ok(()),
        }
    }
}
