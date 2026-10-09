//! The RX front end (datasheet `RFE`, registers 0x70-0x7F): the three LNAs and the RX
//! mixer.
//!
//! This is where the antenna signal enters the chip. One LNA at a time is active
//! ([`LnaControl`]); its gain mode trades sensitivity against the strongest signal it can
//! take without distorting. Its output goes to the RX mixer, driven by the RX PLL's LO
//! buffer for the same LNA path, so changing the LNA also means changing that buffer
//! ([`Lms6002d::select_lna`](super::Lms6002d::select_lna) does both).

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

    /// Datasheet `RDLINT_LNA`; see [`LnaLoad`].
    LnaLoad = 0x79,

    /// Datasheet `LOMIX_GLNA3`; see [`MixerBias`].
    MixerBias = 0x7c,
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
pub(super) enum LnaGain {
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
