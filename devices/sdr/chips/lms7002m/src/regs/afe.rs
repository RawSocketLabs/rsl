//! The analogue front end's converters (`AFE` page) and the reference-clock input buffers
//! (`XBUF` page).

use bnb::{BitEnum, bitfield, u1, u2, u3};

use crate::reg::Reg;
use crate::register::Register;

/// How the receive ADCs sample. `MODE_INTERLEAVE_AFE`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum AdcMode {
    /// Two ADCs, one per channel (`2ADCS`, 0).
    TwoAdcs,

    /// Both ADCs interleaved on one channel (`INTERLEAVED`, 1).
    Interleaved,
}

/// What the first analogue mux routes. `MUX_AFE_1`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum AfeMux1 {
    /// Off (`MUXOFF`, 0).
    Off,

    /// Power detector 1 (`PDET_1`, 1).
    PowerDetector1,

    /// The top bias (`BIAS_TOP`, 2).
    BiasTop,

    /// RSSI 1 (`RSSI1`, 3).
    Rssi1,
}

/// What the second analogue mux routes. `MUX_AFE_2`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum AfeMux2 {
    /// Off (`MUXOFF`, 0).
    Off,

    /// Power detector 2 (`PDET_2`, 1).
    PowerDetector2,

    /// RSSI 1 (`RSSI1`, 2).
    Rssi1,

    /// RSSI 2 (`RSSI2`, 3).
    Rssi2,
}

/// The ADCs and DACs: DAC current, ADC mode, analogue muxes and power-downs.
/// `AFE_0x0082`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Afe {
    /// DAC current selection, a code the pinned source names but does not scale.
    /// `ISEL_DAC_AFE`, bits 15:13; `lms7002m_create` writes 0, every later AFE write 4.
    #[bits(13..=15)]
    dac_current: u3,

    /// `MODE_INTERLEAVE_AFE`, bit 12.
    #[bits(12..=12)]
    adc_mode: AdcMode,

    /// `MUX_AFE_1`, bits 11:10.
    #[bits(10..=11)]
    mux1: AfeMux1,

    /// `MUX_AFE_2`, bits 9:8.
    #[bits(8..=9)]
    mux2: AfeMux2,

    /// The whole AFE powered down. `PD_AFE`, bit 5.
    #[bits(5..=5)]
    powered_down: bool,

    /// Receive converter 1 (channel A) powered down. `PD_RX_AFE1`, bit 4.
    #[bits(4..=4)]
    rx_a_powered_down: bool,

    /// Receive converter 2 (channel B) powered down. `PD_RX_AFE2`, bit 3.
    #[bits(3..=3)]
    rx_b_powered_down: bool,

    /// Transmit converter 1 (channel A) powered down. `PD_TX_AFE1`, bit 2.
    #[bits(2..=2)]
    tx_a_powered_down: bool,

    /// Transmit converter 2 (channel B) powered down. `PD_TX_AFE2`, bit 1.
    #[bits(1..=1)]
    tx_b_powered_down: bool,

    /// The AFE's global enable. `EN_G_AFE`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for Afe {
    const ADDR: Reg = Reg::Afe;
}

/// The reference-clock input buffers, RX and TX (XBUF). `lms7002m_create`'s `txrx_clk`
/// argument routes the TX buffer to the RX one ([`Self::with_tx_second_output`],
/// [`Self::with_rx_from_tx_buffer`]). `XBUF_0x0085`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Xbuf {
    /// The RX buffer self-biased. `SLFB_XBUF_RX`, bit 8.
    #[bits(8..=8)]
    rx_self_biased: bool,

    /// The TX buffer self-biased. `SLFB_XBUF_TX`, bit 7.
    #[bits(7..=7)]
    tx_self_biased: bool,

    /// The RX buffer bypassed. `BYP_XBUF_RX`, bit 6.
    #[bits(6..=6)]
    rx_bypassed: bool,

    /// The TX buffer bypassed. `BYP_XBUF_TX`, bit 5.
    #[bits(5..=5)]
    tx_bypassed: bool,

    /// The TX buffer's second output enabled. `EN_OUT2_XBUF_TX`, bit 4.
    #[bits(4..=4)]
    tx_second_output: bool,

    /// The RX buffer fed from the TX buffer. `EN_TBUFIN_XBUF_RX`, bit 3.
    #[bits(3..=3)]
    rx_from_tx_buffer: bool,

    /// The RX buffer powered down. `PD_XBUF_RX`, bit 2.
    #[bits(2..=2)]
    rx_powered_down: bool,

    /// The TX buffer powered down. `PD_XBUF_TX`, bit 1.
    #[bits(1..=1)]
    tx_powered_down: bool,

    /// The buffers' global enable. `EN_G_XBUF`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for Xbuf {
    const ADDR: Reg = Reg::Xbuf;
}
