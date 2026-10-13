//! The transmit RF front end (`TRF` page): per-channel controls and pad, and the band
//! selection.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics, and docs say how libusdr's `lms7002m_trf_path` sets each.

use bnb::{bitfield, u2, u3, u5};

use super::rfe::NextChannel;
use crate::reg::Reg;
use crate::register::Register;

/// Power-downs, power detector and global enable. `TRF_0x0100`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxFrontEnd {
    /// `EN_LOWBWLOMX_TMX_TRF`, bit 15; libusdr writes 0.
    #[bits(15..=15)]
    low_bandwidth_lo_mixer: bool,

    /// `EN_NEXTTX_TRF`, bit 14.
    #[bits(14..=14)]
    next: NextChannel,

    /// `EN_AMPHF_PDET_TRF`, bits 13:12; libusdr writes 2 ("3" commented out).
    #[bits(12..=13)]
    power_detector_amplifier: u2,

    /// `LOADR_PDET_TRF`, bits 11:10; libusdr writes 1.
    #[bits(10..=11)]
    power_detector_load: u2,

    /// `PD_PDET_TRF`, bit 3; libusdr always sets it.
    #[bits(3..=3)]
    power_detector_powered_down: bool,

    /// `PD_TLOBUF_TRF`, bit 2.
    #[bits(2..=2)]
    lo_buffer_powered_down: bool,

    /// `PD_TXPAD_TRF`, bit 1.
    #[bits(1..=1)]
    pad_powered_down: bool,

    /// The front end's global enable. `EN_G_TRF`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for TxFrontEnd {
    const ADDR: Reg = Reg::TxFrontEnd;
}

/// The TX pad: its losses and loopback. `TRF_0x0101`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxPad {
    /// `F_TXPAD_TRF`, bits 15:13; libusdr writes 3.
    #[bits(13..=15)]
    f: u3,

    /// The loopback path's loss code. `L_LOOPB_TXPAD_TRF`, bits 12:11.
    #[bits(11..=12)]
    loopback_loss: u2,

    /// The pad's linear-stage loss code. `LOSS_LIN_TXPAD_TRF`, bits 10:6.
    #[bits(6..=10)]
    linear_loss: u5,

    /// The pad's main-stage loss code. `LOSS_MAIN_TXPAD_TRF`, bits 5:1.
    #[bits(1..=5)]
    main_loss: u5,

    /// The pad's loopback enabled. `EN_LOOPB_TXPAD_TRF`, bit 0.
    #[bits(0..=0)]
    loopback: bool,
}

impl Register for TxPad {
    const ADDR: Reg = Reg::TxPad;
}

/// Output band selection and LO bias. `TRF_0x0103`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxBand {
    /// Band 1 output selected. `SEL_BAND1_TRF`, bit 11.
    #[bits(11..=11)]
    band1: bool,

    /// Band 2 output selected. `SEL_BAND2_TRF`, bit 10.
    #[bits(10..=10)]
    band2: bool,

    /// `LOBIASN_TXM_TRF`, bits 9:5; libusdr writes 16.
    #[bits(5..=9)]
    lo_bias_n: u5,

    /// `LOBIASP_TXX_TRF`, bits 4:0; libusdr writes 18.
    #[bits(0..=4)]
    lo_bias_p: u5,
}

impl Register for TxBand {
    const ADDR: Reg = Reg::TxBand;
}
