//! The control register: enable, output range and switching mode.

use bnb::{BitEnum, bitfield, u2};

use super::reg::Reg;
use crate::chips::register::Register;

/// How fast the output moves to a new voltage.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum RampRate {
    /// 1.0 V/ms.
    VoltsPerMs1,

    /// 2.5 V/ms.
    VoltsPerMs2_5,

    /// 5.0 V/ms.
    VoltsPerMs5,

    /// 10.0 V/ms.
    VoltsPerMs10,
}

/// Turns the converter on and sets its range and switching. `CONTROL`, §8.6.1.2.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Control {
    /// Use the high output range (2.025 V to 5.2 V) instead of the low (1.8 V to
    /// 4.975 V). `RANGE`, bit 6; reset 0.
    #[bits(6..=6)]
    high_range: bool,

    /// Run the converter; the TPS63811 starts off. `ENABLE`, bit 5.
    #[bits(5..=5)]
    enabled: bool,

    /// Always switch in PWM. `FPWM`, bit 3; reset 0.
    #[bits(3..=3)]
    forced_pwm: bool,

    /// Switch in PWM while the output ramps to a new voltage. `RPWM`, bit 2; reset 0.
    #[bits(2..=2)]
    ramp_pwm: bool,

    /// Output ramp rate. `SLEW`, bits 1:0; reset 1.0 V/ms.
    #[bits(0..=1)]
    ramp_rate: RampRate,
}
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}
