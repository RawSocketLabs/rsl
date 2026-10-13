//! The receive RF front end (`RFE` page), one bank per channel.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics, and docs say how libusdr's `lms7002m_rfe_path` sets each.

use bnb::{BitEnum, bitfield, u1, u2, u4, u5};

use crate::reg::Reg;
use crate::register::Register;

/// Which LNA input the receive front end uses. `SEL_PATH_RFE`; libusdr's `RFE_*` paths
/// have the same codes.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum RxPath {
    /// No input (`NONE`, 0; `RFE_NONE`).
    None,

    /// The high-band LNA (`LNAH`, 1; `RFE_LNAH`).
    LnaH,

    /// The low-band LNA (`LNAL`, 2; `RFE_LNAL`).
    LnaL,

    /// The wide-band LNA (`LNAW`, 3; `RFE_LNAW`).
    LnaW,
}

/// Whether a channel's front end also serves the other channel (`EN_NEXTRX_RFE`,
/// `EN_NEXTTX_TRF`): the YAML's `SISO`/`MIMO` options.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum NextChannel {
    /// `SISO`, 0.
    Siso,

    /// `MIMO`, 1: libusdr sets it on channel A while channel B's front end is enabled.
    Mimo,
}

/// Power-downs and global enable. `RFE_0x010C`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxFrontEnd {
    /// `CDC_I_RFE`, bits 15:12; libusdr writes 8.
    #[bits(12..=15)]
    cdc_i: u4,

    /// `CDC_Q_RFE`, bits 11:8; libusdr writes 8.
    #[bits(8..=11)]
    cdc_q: u4,

    /// `PD_LNA_RFE`, bit 7; libusdr sets it for an enabled channel in RF loopback.
    #[bits(7..=7)]
    lna_powered_down: bool,

    /// `PD_RLOOPB_1_RFE`, bit 6.
    #[bits(6..=6)]
    loopback1_powered_down: bool,

    /// `PD_RLOOPB_2_RFE`, bit 5.
    #[bits(5..=5)]
    loopback2_powered_down: bool,

    /// `PD_MXLOBUF_RFE`, bit 4.
    #[bits(4..=4)]
    mixer_lo_buffer_powered_down: bool,

    /// `PD_QGEN_RFE`, bit 3.
    #[bits(3..=3)]
    qgen_powered_down: bool,

    /// `PD_RSSI_RFE`, bit 2; libusdr always sets it.
    #[bits(2..=2)]
    rssi_powered_down: bool,

    /// `PD_TIA_RFE`, bit 1.
    #[bits(1..=1)]
    tia_powered_down: bool,

    /// The front end's global enable. `EN_G_RFE`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for RxFrontEnd {
    const ADDR: Reg = Reg::RxFrontEnd;
}

/// Input selection and input switches. `RFE_0x010D`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxInput {
    /// `SEL_PATH_RFE`, bits 8:7.
    #[bits(7..=8)]
    path: RxPath,

    /// `EN_DCOFF_RXFE_RFE`, bit 6; libusdr always sets it.
    #[bits(6..=6)]
    dc_offset_enabled: bool,

    /// `EN_INSHSW_LB1_RFE`, bit 4.
    #[bits(4..=4)]
    inshsw_lb1: bool,

    /// `EN_INSHSW_LB2_RFE`, bit 3.
    #[bits(3..=3)]
    inshsw_lb2: bool,

    /// `EN_INSHSW_L_RFE`, bit 2.
    #[bits(2..=2)]
    inshsw_l: bool,

    /// `EN_INSHSW_W_RFE`, bit 1.
    #[bits(1..=1)]
    inshsw_w: bool,

    /// `EN_NEXTRX_RFE`, bit 0.
    #[bits(0..=0)]
    next: NextChannel,
}

impl Register for RxInput {
    const ADDR: Reg = Reg::RxInput;
}

/// Bias currents, as codes the pinned source does not scale. `RFE_0x010F`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxBias {
    /// `ICT_LOOPB_RFE`, bits 14:10; libusdr writes 3 in RF loopback, else 0.
    #[bits(10..=14)]
    loopback: u5,

    /// `ICT_TIAMAIN_RFE`, bits 9:5; libusdr writes 2.
    #[bits(5..=9)]
    tia_main: u5,

    /// `ICT_TIAOUT_RFE`, bits 4:0; libusdr writes 2.
    #[bits(0..=4)]
    tia_out: u5,
}

impl Register for RxBias {
    const ADDR: Reg = Reg::RxBias;
}
