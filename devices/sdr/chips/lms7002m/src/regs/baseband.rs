//! The receive and transmit baseband (`RBB`, `TBB` pages): filters, the receive PGA and
//! the transmit amplifier, one bank per channel.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics, and docs say what libusdr's `lms7002m_rbb_*` and `lms7002m_tbb_*` write.

use bnb::{BitEnum, bitfield, u2, u3, u5, u6, u11};

use crate::reg::Reg;
use crate::register::Register;

/// Loopbacks, power-downs and the global enable. `RBB_0x0115`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbControl {
    /// The TX baseband looped into the high-band filter. `EN_LB_LPFH_RBB`, bit 15.
    #[bits(15..=15)]
    loopback_high: bool,

    /// The TX baseband looped into the low-band filter. `EN_LB_LPFL_RBB`, bit 14.
    #[bits(14..=14)]
    loopback_low: bool,

    /// `PD_LPFH_RBB`, bit 3.
    #[bits(3..=3)]
    high_filter_powered_down: bool,

    /// `PD_LPFL_RBB`, bit 2.
    #[bits(2..=2)]
    low_filter_powered_down: bool,

    /// `PD_PGA_RBB`, bit 1.
    #[bits(1..=1)]
    pga_powered_down: bool,

    /// The baseband's global enable. `EN_G_RBB`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for RbbControl {
    const ADDR: Reg = Reg::RbbControl;
}

/// The high-band filter's controls and the filters' shared resistor. `RBB_0x0116`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbHighFilter {
    /// `R_CTL_LPF_RBB`, bits 15:11; libusdr writes 16.
    #[bits(11..=15)]
    resistor: u5,

    /// `RCC_CTL_LPFH_RBB`, bits 10:8.
    #[bits(8..=10)]
    rcc: u3,

    /// `C_CTL_LPFH_RBB`, bits 7:0.
    #[bits(0..=7)]
    capacitor: u8,
}

impl Register for RbbHighFilter {
    const ADDR: Reg = Reg::RbbHighFilter;
}

/// The low-band filter's controls. `RBB_0x0117`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbLowFilter {
    /// `RCC_CTL_LPFL_RBB`, bits 13:11.
    #[bits(11..=13)]
    rcc: u3,

    /// `C_CTL_LPFL_RBB`, bits 10:0.
    #[bits(0..=10)]
    capacitor: u11,
}

impl Register for RbbLowFilter {
    const ADDR: Reg = Reg::RbbLowFilter;
}

/// What the receive PGA takes. `INPUT_CTL_PGA_RBB`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
pub enum PgaInput {
    /// The low-band filter (`LPFL`, 0).
    LowFilter,

    /// The high-band filter (`LPFH`, 1).
    HighFilter,

    /// No filter (`BYPASS`, 2).
    Bypass,

    /// The TX baseband, a loopback (`TBB`, 3).
    Tbb,

    /// The power detector (`PDET`, 4).
    PowerDetector,

    /// Code 5, not named.
    Reserved5,

    /// Code 6, not named.
    Reserved6,

    /// Code 7, not named.
    Reserved7,
}

/// The PGA's input and the filters' bias. `RBB_0x0118`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbInput {
    /// `INPUT_CTL_PGA_RBB`, bits 15:13.
    #[bits(13..=15)]
    pga_input: PgaInput,

    /// `ICT_LPF_IN_RBB`, bits 9:5; libusdr writes 24.
    #[bits(5..=9)]
    filter_in_bias: u5,

    /// `ICT_LPF_OUT_RBB`, bits 4:0; libusdr writes 24.
    #[bits(0..=4)]
    filter_out_bias: u5,
}

impl Register for RbbInput {
    const ADDR: Reg = Reg::RbbInput;
}

/// The receive PGA's gain and bias. `RBB_0x0119`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbPga {
    /// `OSW_PGA_RBB`, bit 15.
    #[bits(15..=15)]
    output_switch: bool,

    /// `ICT_PGA_OUT_RBB`, bits 14:10; libusdr writes 20.
    #[bits(10..=14)]
    out_bias: u5,

    /// `ICT_PGA_IN_RBB`, bits 9:5; libusdr writes 20.
    #[bits(5..=9)]
    in_bias: u5,

    /// The gain in dB, 0–31. `G_PGA_RBB`, bits 4:0.
    #[bits(0..=4)]
    gain: u5,
}

impl Register for RbbPga {
    const ADDR: Reg = Reg::RbbPga;
}

/// The receive PGA's feedback. `RBB_0x011A`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RbbPgaFeedback {
    /// `RCC_CTL_PGA_RBB`, bits 13:9.
    #[bits(9..=13)]
    rcc: u5,

    /// `C_CTL_PGA_RBB`, bits 7:0.
    #[bits(0..=7)]
    capacitor: u8,
}

impl Register for RbbPgaFeedback {
    const ADDR: Reg = Reg::RbbPgaFeedback;
}

/// The transmit baseband's loopbacks, power-downs and global enable. `TBB_0x0105`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TbbControl {
    /// `STATPULSE_TBB`, bit 15.
    #[bits(15..=15)]
    static_pulse: bool,

    /// `LOOPB_SWAP_IQ_TBB`, bit 14.
    #[bits(14..=14)]
    loopback_swap_iq: bool,

    /// The loopback: 0 `NORMAL`, 1 `LB_DAC`, 2 `LB_LPF_LAD`, 3 `LB_TBB_OUT`. `LOOPB_TBB`,
    /// bits 13:12.
    #[bits(12..=13)]
    loopback: u2,

    /// `PD_LPFH_TBB`, bit 4.
    #[bits(4..=4)]
    high_filter_powered_down: bool,

    /// `PD_LPFIAMP_TBB`, bit 3.
    #[bits(3..=3)]
    amplifier_powered_down: bool,

    /// `PD_LPFLAD_TBB`, bit 2.
    #[bits(2..=2)]
    ladder_filter_powered_down: bool,

    /// `PD_LPFS5_TBB`, bit 1.
    #[bits(1..=1)]
    s5_filter_powered_down: bool,

    /// The baseband's global enable. `EN_G_TBB`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for TbbControl {
    const ADDR: Reg = Reg::TbbControl;
}

/// The transmit amplifier's gain and bias. `TBB_0x0108`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TbbAmplifier {
    /// The amplifier's gain code, 1–63. `CG_IAMP_TBB`, bits 15:10.
    #[bits(10..=15)]
    gain: u6,

    /// `ICT_IAMP_FRP_TBB`, bits 9:5; libusdr writes 12.
    #[bits(5..=9)]
    reference_bias: u5,

    /// `ICT_IAMP_GG_FRP_TBB`, bits 4:0; libusdr writes 12.
    #[bits(0..=4)]
    gain_bias: u5,
}

impl Register for TbbAmplifier {
    const ADDR: Reg = Reg::TbbAmplifier;
}

/// The transmit filters' resistor calibration. `TBB_0x0109`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TbbFilterCal {
    /// `RCAL_LPFH_TBB`, bits 15:8.
    #[bits(8..=15)]
    high: u8,

    /// `RCAL_LPFLAD_TBB`, bits 7:0.
    #[bits(0..=7)]
    ladder: u8,
}

impl Register for TbbFilterCal {
    const ADDR: Reg = Reg::TbbFilterCal;
}

/// The transmit baseband's test input and ladder controls. `TBB_0x010A`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TbbTest {
    /// The test input: 0 `DISABLED`, 1 `LPFH`, 2 `LPFL`, 3 `CURAMP`. `TSTIN_TBB`, bits
    /// 15:14.
    #[bits(14..=15)]
    test_input: u2,

    /// `BYPLADDER_TBB`, bit 13.
    #[bits(13..=13)]
    ladder_bypassed: bool,

    /// `CCAL_LPFLAD_TBB`, bits 12:8.
    #[bits(8..=12)]
    ladder_capacitor: u5,

    /// `RCAL_LPFS5_TBB`, bits 7:0.
    #[bits(0..=7)]
    s5_resistor: u8,
}

impl Register for TbbTest {
    const ADDR: Reg = Reg::TbbTest;
}

/// The S5 filter's bypass. `TBB_LPFS5` (0x010B).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TbbS5 {
    /// `BYPASS`, bit 0; libusdr sets it.
    #[bits(0..=0)]
    bypassed: bool,
}

impl Register for TbbS5 {
    const ADDR: Reg = Reg::TbbS5;
}
