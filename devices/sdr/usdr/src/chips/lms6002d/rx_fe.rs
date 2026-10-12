//! The RX front end (datasheet `RFE`, registers 0x70-0x7F): the three LNAs, the RX
//! mixer and RXVGA1, the mixer's transimpedance amplifier (TIA).
//!
//! This is where the antenna signal enters the chip. One LNA at a time is active
//! ([`LnaControl`]); its gain mode trades sensitivity against the strongest signal it can
//! take without distorting. Its output goes to the RX mixer, driven by the RX PLL's LO
//! buffer for the same LNA path, so changing the LNA also means changing that buffer
//! ([`Lms6002d::select_lna`](super::Lms6002d::select_lna) does both). RXVGA1's feedback
//! resistor sets its gain ([`TiaFeedback`]).

use bnb::{BitEnum, bitfield, u2, u4, u6, u7};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// RX front-end register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `CTRL`; see [`Control`].
    Control = 0x70,

    /// Datasheet `GAIN_LNA_SEL`; see [`LnaControl`].
    LnaControl = 0x75,

    /// Datasheet `IN1SEL_DCI`; see [`MixerInput`].
    MixerInput = 0x71,

    /// Datasheet `RFB_TIA`; see [`TiaFeedback`].
    TiaFeedback = 0x76,

    /// Datasheet `RDLINT_LNA`; see [`LnaLoad`].
    LnaLoad = 0x79,

    /// Datasheet `LOMIX_GLNA3`; see [`MixerBias`].
    MixerBias = 0x7c,

    /// Datasheet `PD`; see [`PowerDown`].
    PowerDown = 0x7d,
}

impl BlockReg for Reg {}

/// An LNA input path. On the uSDR, LNA1 is the wideband input.
///
/// The uSDR wiring, from libusdr's `usdr_ctrl.c` band table; the RX switch is
/// [`Gpo::RxSwitch`](crate::fpga::Gpo::RxSwitch).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum Lna {
    /// No LNA.
    None,

    /// LNA1: 0.3 to 2.8 GHz through the B0322J5050AHF balun, RX switch 0. libusdr's
    /// `LNAW` path and the power-up default.
    Lna1,

    /// LNA2: 1.5 to 3.8 GHz through the 3600BL14M050 balun, RX switch 1. libusdr's `LNAH`
    /// path.
    Lna2,

    /// LNA3: the low band below 230 MHz, through the board's upconverting mixer, RX switch
    /// 1; libusdr's wiring table also names the M.2 RF port. libusdr's `LNAL` path.
    Lna3,
}

/// LNA gain mode.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum LnaGain {
    /// Maximum gain, LNA3 only.
    Lna3Max,

    /// LNA bypassed, LNA1 and LNA2 only.
    Bypassed,

    /// Mid gain.
    Mid,

    /// Maximum gain.
    Max,
}

/// Powers and enables the RX front end. Datasheet `CTRL`, register 0x70.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Control {
    /// Take control signals from the test-mode registers instead of decoding them.
    /// `RFE_DECODE`, bit 1; reset 0.
    #[bits(1..=1)]
    test_mode_controls: bool,

    /// Power the RX front-end modules. `RFE_EN`, bit 0; reset 1.
    #[bits(0..=0)]
    enabled: bool,
}
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}

/// LNA gain, which LNA is active, and its input capacitance. Datasheet `GAIN_LNA_SEL`,
/// register 0x75.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LnaControl {
    /// Gain mode. `G_LNA`, bits 7:6; reset maximum.
    #[bits(6..=7)]
    gain: LnaGain,

    /// The active LNA. `LNASEL`, bits 5:4; reset LNA1.
    #[bits(4..=5)]
    active: Lna,

    /// Extra capacitance across the input transistors' base-emitter, easing matching at
    /// low frequencies; LNA1 and LNA2 only. `CBE_LNA`, bits 3:0.
    #[bits(0..=3)]
    input_capacitance: u4,
}
impl Register for LnaControl {
    type Map = Reg;
    const ADDR: Reg = Reg::LnaControl;
}

/// RXVGA1's gain, set by the feedback resistor of the TIA after the mixer. Datasheet
/// `RFB_TIA`, register 0x76.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TiaFeedback {
    /// Feedback resistor code; the gain rises with it: 120 gives 30 dB, 102 gives 19 dB
    /// and 2 gives 5 dB. `RFB_TIA`, bits 6:0; reset 120.
    #[bits(0..=6)]
    resistance: u7,
}
impl Register for TiaFeedback {
    type Map = Reg;
    const ADDR: Reg = Reg::TiaFeedback;
}

impl TiaFeedback {
    /// The code for `db` of gain, by libusdr's formula (`_lms6002d_rxvga1_db_to_int`):
    /// `127.5 − 127 / 10^((dB − 5) / 20)`, truncated, with `db` held to 5 at the bottom and
    /// anything above 30 taken as 30.17. That puts the YAML's 19 and 30 dB points at their
    /// codes, 102 and 120; its 5 dB point is code 0 here, not 2.
    pub(super) fn for_gain_db(db: i32) -> Self {
        let db = match db {
            ..5 => 5.0,
            31.. => 30.17,
            _ => f64::from(db),
        };
        let code = 127.5 - 127.0 / 10_f64.powf((db - 5.0) / 20.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "libusdr truncates the double to int8_t; over 5..=30.17 dB it is 0..=120"
        )]
        let code = code as u8;
        // libusdr's register macro keeps the low seven bits.
        Self::new().with_resistance(u7::new(code & 0x7f))
    }
}

/// The on-chip LNA load resistor, for LNA1 and LNA2 in internal-load mode. Datasheet
/// `RDLINT_LNA`, register 0x79.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LnaLoad {
    /// Load resistance code. `RDLINT_LNA`, bits 5:0.
    #[bits(0..=5)]
    resistance: u6,
}
impl Register for LnaLoad {
    type Map = Reg;
    const ADDR: Reg = Reg::LnaLoad;
}

impl LnaLoad {
    /// Lime's recommended load, code 0x37, from the LMS6002D FAQ v1.0r12, 5.27 (as libusdr
    /// writes it). Neither libusdr nor its register map gives the resistance per code.
    pub(super) const LIME_RECOMMENDED: Self = Self::new().with_resistance(u6::new(0x37));
}

/// The mixer's input and the I channel's DC-offset trim. Datasheet `IN1SEL_DCI`,
/// register 0x71.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MixerInput {
    /// Feed the mixer from the on-chip LNA (input 1) rather than the pads (input 2).
    /// `IN1SEL_MIX`, bit 7; reset 1.
    #[bits(7..=7)]
    from_lna: bool,

    /// I-channel DC offset, sign-magnitude: bit 6 the sign, bits 5:0 the magnitude.
    /// `DCOFF_I`, bits 6:0.
    #[bits(0..=6)]
    dc_offset_i: u7,
}
impl Register for MixerInput {
    type Map = Reg;
    const ADDR: Reg = Reg::MixerInput;
}

/// The mixer's LO bias and input termination, and LNA3's fine gain. Datasheet
/// `LOMIX_GLNA3`, register 0x7C.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MixerBias {
    /// LO bias of the mixer, for linearity. `LOBN_MIX`, bits 6:3; reset 3.
    #[bits(3..=6)]
    lo_bias: u4,

    /// Terminate the external mixer input. `RINEN_MIX`, bit 2; reset 0.
    #[bits(2..=2)]
    input_terminated: bool,

    /// LNA3 fine gain, 1 dB per step. `G_FINE_LNA3`, bits 1:0.
    #[bits(0..=1)]
    lna3_fine_gain: u2,
}
impl Register for MixerBias {
    type Map = Reg;
    const ADDR: Reg = Reg::MixerBias;
}

/// Powers individual RX front-end stages down. Datasheet `PD`, register 0x7D.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PowerDown {
    /// Power the TIA (RXVGA1) down. `TIA`, bit 3; reset 0.
    #[bits(3..=3)]
    tia_off: bool,

    /// Power the mixer's LO buffer down. `MXLOB`, bit 2; reset 0.
    #[bits(2..=2)]
    lo_buffer_off: bool,

    /// Power the mixer down. `MIX`, bit 1; reset 0.
    #[bits(1..=1)]
    mixer_off: bool,

    /// Power the LNA down. `LNA`, bit 0; reset 0.
    #[bits(0..=0)]
    lna_off: bool,
}
impl Register for PowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::PowerDown;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resistor code for `db`.
    fn code(db: i32) -> u8 {
        TiaFeedback::for_gain_db(db).resistance().value()
    }

    #[test]
    fn rxvga1_codes_meet_the_yaml_gain_points() {
        // libusdr's register YAML: 120 gives 30 dB, 102 gives 19 dB.
        assert_eq!(code(30), 120);
        assert_eq!(code(19), 102);
    }

    #[test]
    fn rxvga1_gain_is_held_to_5_db_and_to_the_30_db_code_above() {
        assert_eq!(code(5), 0);
        assert_eq!(code(i32::MIN), code(5));
        // libusdr's 30.17 dB case gives 120.5, truncated to the 30 dB code, so it cannot
        // be told apart from 30 dB.
        assert_eq!(code(31), code(30));
        assert_eq!(code(i32::MAX), code(30));
    }
}
