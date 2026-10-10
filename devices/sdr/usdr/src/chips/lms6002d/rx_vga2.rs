//! The second RX variable-gain amplifier (datasheet `RxVGA2`, registers 0x60-0x6F).
//!
//! RXVGA2 is the last gain stage before the ADC, after the channel filter, 0 to 30 dB in
//! libusdr's range, and so sets how fully the signal fills the ADC's input range.
//! Power-up writes its output common-mode voltage ([`Control`]) to the value from Lime's
//! LMS6002D FAQ, 5.27.

use bnb::{BitEnum, bitfield, u4};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// RXVGA2 register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `CTRL`; see [`Control`].
    Control = 0x64,

    /// Datasheet `GAIN`; see [`Gain`].
    Gain = 0x68,

    /// Datasheet `PD_CALIB`; see [`CalibrationPower`].
    CalibrationPower = 0x6e,
}

impl BlockReg for Reg {}

/// Powers RXVGA2 and sets its output common mode. Datasheet `CTRL`, register 0x64.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Control {
    /// Output common-mode voltage: bit 3 is the sign, bits 2:0 the magnitude in 40 mV
    /// steps. `CMV`, bits 5:2.
    #[bits(2..=5)]
    common_mode: u4,

    /// Power the RXVGA2 modules. `EN`, bit 1; reset 1.
    #[bits(1..=1)]
    enabled: bool,

    /// Take control signals from the test-mode registers instead of decoding them.
    /// `DECODE`, bit 0; reset 0.
    #[bits(0..=0)]
    test_mode_controls: bool,
}
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}

impl Control {
    /// Lime's recommended setting, 0x36, from the LMS6002D FAQ v1.0r12, 5.27 (as libusdr
    /// writes it): powered, common-mode code `0b1101`. libusdr's YAML scale cannot place
    /// that code: 40 mV steps up from its 620 mV at `0b1000` give 820 mV, but down from its
    /// 860 mV at `0b1111` give 780 mV.
    pub(super) const LIME_RECOMMENDED: Self = Self::new()
        .with_common_mode(u4::new(0b1101))
        .with_enabled(true);

    /// libusdr's RX power-up value, 0x1F (`lms6002d_rxvga2_enable(true)`): powered,
    /// common-mode code 7, and control signals from the test-mode registers. libusdr gives
    /// no reason for leaving the FAQ's common mode or for setting `DECODE`.
    pub(super) const RX_POWER_UP: Self = Self::new()
        .with_common_mode(u4::new(7))
        .with_enabled(true)
        .with_test_mode_controls(true);

    /// [`Self::RX_POWER_UP`] powered down, 0x1D (`lms6002d_rxvga2_enable(false)`).
    pub(super) const RX_POWER_DOWN: Self = Self::RX_POWER_UP.with_enabled(false);
}

/// The second VGA's two stages' gains, 3 dB per step. Datasheet `GAIN`, register 0x68.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Gain {
    /// VGA2B gain. `INB_VAL`, bits 7:4.
    #[bits(4..=7)]
    b: u4,

    /// VGA2A gain. `INA_VAL`, bits 3:0.
    #[bits(0..=3)]
    a: u4,
}
impl Register for Gain {
    type Map = Reg;
    const ADDR: Reg = Reg::Gain;
}

/// Power for the DC-calibration comparators of each VGA2 stage. Datasheet `PD_CALIB`,
/// register 0x6E.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CalibrationPower {
    /// Power VGA2B's comparator down. `PD_CAL_VGA2B`, bit 7; reset 0.
    #[bits(7..=7)]
    b_off: bool,

    /// Power VGA2A's comparator down. `PD_CAL_VGA2A`, bit 6; reset 0.
    #[bits(6..=6)]
    a_off: bool,
}
impl Register for CalibrationPower {
    type Map = Reg;
    const ADDR: Reg = Reg::CalibrationPower;
}
