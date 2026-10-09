//! The two output-voltage registers, one per VSEL pin level.

use super::reg::Reg;
use crate::chips::register::IndexedRegister;

/// Which output-voltage register, chosen by the VSEL pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vsel {
    /// VSEL low: `VOUT1`.
    Low,
    /// VSEL high: `VOUT2`.
    High,
}

/// An output voltage in the low range, as its `VOUTn` code (1.8 V plus 25 mV per step).
/// Built in const context, so an out-of-range board constant fails to compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputVoltage(u8);

impl OutputVoltage {
    /// Lowest low-range output, in millivolts.
    const MIN_MV: u32 = 1800;
    /// Highest low-range output, in millivolts.
    const MAX_MV: u32 = 4975;

    /// The code for `millivolts`, rounded down to a 25 mV step.
    ///
    /// # Panics
    ///
    /// Outside 1800..=4975 mV (at compile time when used in a `const`).
    pub(crate) const fn from_millivolts(millivolts: u32) -> Self {
        assert!(
            millivolts >= Self::MIN_MV && millivolts <= Self::MAX_MV,
            "TPS6381x output out of range"
        );
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check bounds the code to 127"
        )]
        let code = ((millivolts - Self::MIN_MV) / 25) as u8;
        Self(code)
    }
}

impl IndexedRegister for OutputVoltage {
    type Map = Reg;
    type Index = Vsel;

    fn addr(vsel: Vsel) -> Reg {
        match vsel {
            Vsel::Low => Reg::VoltageVselLow,
            Vsel::High => Reg::VoltageVselHigh,
        }
    }
}

impl From<OutputVoltage> for u8 {
    fn from(value: OutputVoltage) -> Self {
        value.0
    }
}
