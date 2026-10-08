//! The ADC/DAC interface (datasheet `AFE`, registers 0x57-0x5F).

use bnb::{BitEnum, bitfield, u2};

use super::spi::BlockReg;
use crate::chips::register::bitfield_register;

/// AFE register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `RX_CTRL2`: ADC reference gain (`GAIN_ADJ`, bits 6:5), common mode
    /// (`CM_ADJ`, bits 4:3) and reference buffer boost (`BUF_BOOST`, bits 2:1). Written as
    /// a raw byte: libusdr's value also sets bit 0, which the map does not define.
    AdcReference = 0x59,
    /// Datasheet `MISC_CTRL`; see [`Interface`].
    Interface = 0x5a,
}

impl BlockReg for Reg {}

/// Extra non-overlap between the converter clock phases.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum ClockNonOverlap {
    /// Nominal.
    Nominal,
    /// +450 ps.
    Plus450ps,
    /// +150 ps.
    Plus150ps,
    /// +300 ps.
    Plus300ps,
}

/// Framing and clocking of the digital IQ interface. Datasheet `MISC_CTRL`, register 0x5A.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Interface {
    /// RX frame-sync polarity at a frame start; the YAML does not say which level 1 selects.
    /// `RXPOL`, bit 7.
    #[bits(7..=7)]
    rx_frame_sync_polarity: bool,
    /// Send RX samples Q first instead of I first. `RXIQ`, bit 6; reset 0.
    #[bits(6..=6)]
    rx_q_first: bool,
    /// Clock the DAC on the negative edge. `DAC_EDGE`, bit 5; reset 1.
    #[bits(5..=5)]
    dac_negative_edge: bool,
    /// TX frame-sync polarity at a frame start; the YAML does not say which level 1 selects.
    /// `TXPOL`, bit 4.
    #[bits(4..=4)]
    tx_frame_sync_polarity: bool,
    /// Take TX samples Q first instead of I first. `TXIQ`, bit 3; reset 0.
    #[bits(3..=3)]
    tx_q_first: bool,
    /// Clock the ADC on the negative edge. `ADC_EDGE`, bit 2; reset 1.
    #[bits(2..=2)]
    adc_negative_edge: bool,
    /// Converter clock non-overlap. `CLOCK_ADJ`, bits 1:0.
    #[bits(0..=1)]
    clock_non_overlap: ClockNonOverlap,
}
bitfield_register!(Interface => Reg::Interface);
