//! The synthesizers (`SXX` page): the per-channel `EN_DIR` bits and the RX (`SXR`, bank A)
//! and TX (`SXT`, bank B) PLLs, their dividers, VCOs and tuning comparators (read as
//! [`VcoComparators`](crate::VcoComparators)).
//!
//! The pinned source names the PLL fields without describing them; names follow the
//! mnemonics, and docs say what libusdr's `lms7002m_sxx_tune` writes.

use bnb::{BitEnum, bitfield, u2, u3, u4, u5, u6, u10};

use crate::reg::Reg;
use crate::register::Register;

/// One `EN_DIR` bit per block. The pinned source names them only; libusdr sets the RFE bit
/// while a channel's receive front end is enabled, and the TRF bit from the same RFE
/// enables (`lms7002m_trf_path` reads `rfe[i].en`, not `trf[i].en`). `SXX_0x0124`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnDir {
    /// `EN_DIR_SXX`, bit 4.
    #[bits(4..=4)]
    sxx: bool,

    /// `EN_DIR_RBB`, bit 3.
    #[bits(3..=3)]
    rbb: bool,

    /// `EN_DIR_RFE`, bit 2.
    #[bits(2..=2)]
    rfe: bool,

    /// `EN_DIR_TBB`, bit 1.
    #[bits(1..=1)]
    tbb: bool,

    /// `EN_DIR_TRF`, bit 0.
    #[bits(0..=0)]
    trf: bool,
}

impl Register for EnDir {
    const ADDR: Reg = Reg::EnDir;
}

/// Resets, power-downs and enables of the selected synthesizer. `SXX_0x011C`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxControl {
    /// `RESET_N`, bit 15; libusdr sets it.
    #[bits(15..=15)]
    running: bool,

    /// `SPDUP_VCO`, bit 14.
    #[bits(14..=14)]
    vco_speed_up: bool,

    /// `BYPLDO_VCO`, bit 13.
    #[bits(13..=13)]
    vco_ldo_bypassed: bool,

    /// `EN_COARSEPLL`, bit 12.
    #[bits(12..=12)]
    coarse_pll: bool,

    /// `CURLIM_VCO`, bit 11; libusdr sets it.
    #[bits(11..=11)]
    vco_current_limit: bool,

    /// `EN_DIV2_DIVPROG`, bit 10; libusdr sets it.
    #[bits(10..=10)]
    divprog_div2: bool,

    /// `EN_INTONLY_SDM`, bit 9.
    #[bits(9..=9)]
    integer_only: bool,

    /// `EN_SDM_CLK`, bit 8; libusdr sets it.
    #[bits(8..=8)]
    sdm_clock: bool,

    /// `PD_FBDIV`, bit 7.
    #[bits(7..=7)]
    feedback_divider_powered_down: bool,

    /// `PD_LOCH_T2RBUF`, bit 6: the TX synthesizer's buffer to the RX side powered down.
    /// libusdr clears it only for TDD (`lochen`).
    #[bits(6..=6)]
    lo_to_rx_buffer_powered_down: bool,

    /// `PD_CP`, bit 5.
    #[bits(5..=5)]
    charge_pump_powered_down: bool,

    /// `PD_FDIV`, bit 4.
    #[bits(4..=4)]
    forward_divider_powered_down: bool,

    /// `PD_SDM`, bit 3.
    #[bits(3..=3)]
    sdm_powered_down: bool,

    /// `PD_VCO_COMP`, bit 2.
    #[bits(2..=2)]
    comparator_powered_down: bool,

    /// `PD_VCO`, bit 1.
    #[bits(1..=1)]
    vco_powered_down: bool,

    /// The synthesizer's global enable. `EN_G`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for SxxControl {
    const ADDR: Reg = Reg::SxxControl;
}

/// The feedback divider's fraction, bits 15:0 of 20. `SXX_0x011D` (`FRAC_SDM`).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxFracLow {
    /// `FRAC_SDM[15:0]`.
    #[bits(0..=15)]
    fraction: u16,
}

impl Register for SxxFracLow {
    const ADDR: Reg = Reg::SxxFracLow;
}

/// The feedback divider's integer part and the fraction's top bits. `SXX_0x011E`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxIntFrac {
    /// The integer divider minus four. `INT_SDM`, bits 13:4.
    #[bits(4..=13)]
    integer: u10,

    /// `FRAC_SDM[19:16]`, bits 3:0.
    #[bits(0..=3)]
    fraction_high: u4,
}

impl Register for SxxIntFrac {
    const ADDR: Reg = Reg::SxxIntFrac;
}

/// The LO divider and test controls. `SXX_0x011F`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxLoDivider {
    /// `PW_DIV2_LOCH`, bits 14:12; libusdr writes 3.
    #[bits(12..=14)]
    div2_power: u3,

    /// `PW_DIV4_LOCH`, bits 11:9; libusdr writes 3.
    #[bits(9..=11)]
    div4_power: u3,

    /// The LO divider. libusdr writes how many times it doubled the requested LO to reach
    /// a VCO's range (6 while tuning). `DIV_LOCH`, bits 8:6.
    #[bits(6..=8)]
    divider: u3,

    /// `TST_SX`, bits 5:3.
    #[bits(3..=5)]
    test: u3,

    /// `SEL_SDMCLK`, bit 2.
    #[bits(2..=2)]
    sdm_clock_from_reference: bool,

    /// `SX_DITHER_EN`, bit 1.
    #[bits(1..=1)]
    dither: bool,

    /// `REV_SDMCLK`, bit 0.
    #[bits(0..=0)]
    reverse_sdm_clock: bool,
}

impl Register for SxxLoDivider {
    const ADDR: Reg = Reg::SxxLoDivider;
}

/// The VCO regulator and bias. `SXX_0x0120`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxVcoBias {
    /// `VDIV_VCO`, bits 15:8; libusdr writes 204.
    #[bits(8..=15)]
    regulator_divider: u8,

    /// `ICT_VCO`, bits 7:0; libusdr writes 192.
    #[bits(0..=7)]
    bias: u8,
}

impl Register for SxxVcoBias {
    const ADDR: Reg = Reg::SxxVcoBias;
}

/// Which of the synthesizer's three VCOs runs. `SEL_VCO`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum SxxVcoSelect {
    /// The low VCO (`VCOL`, 0).
    Low,

    /// The middle VCO (`VCOM`, 1).
    Middle,

    /// The high VCO (`VCOH`, 2).
    High,

    /// Code 3, which the source does not name.
    Reserved,
}

/// The VCO's regulator, capacitor bank and selection. `SXX_0x0121`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxVco {
    /// `RSEL_LDO_VCO`, bits 15:11; libusdr writes 16.
    #[bits(11..=15)]
    regulator: u5,

    /// The capacitor bank code. `CSW_VCO`, bits 10:3.
    #[bits(3..=10)]
    cap: u8,

    /// `SEL_VCO`, bits 2:1.
    #[bits(1..=2)]
    select: SxxVcoSelect,

    /// `COARSE_START`, bit 0.
    #[bits(0..=0)]
    coarse_start: bool,
}

impl Register for SxxVco {
    const ADDR: Reg = Reg::SxxVco;
}

/// The charge pump. `SXX_0x0122`.
///
/// libusdr ORs bit 13 into its write, which the source documents no field for; it is kept
/// as the raw bit [`SxxChargePump::undocumented_13`].
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SxxChargePump {
    /// Bit 13: no field in the source; libusdr's `lms7002m_sxx_tune` sets it.
    #[bits(13..=13)]
    undocumented_13: bool,

    /// `REVPH_PFD`, bit 12.
    #[bits(12..=12)]
    reverse_phase: bool,

    /// `IOFFSET_CP`, bits 11:6; libusdr writes 20.
    #[bits(6..=11)]
    offset_current: u6,

    /// `IPULSE_CP`, bits 5:0; libusdr writes 20.
    #[bits(0..=5)]
    pulse_current: u6,
}

impl Register for SxxChargePump {
    const ADDR: Reg = Reg::SxxChargePump;
}
