//! The ADC/DAC interface (datasheet `AFE`, registers 0x57-0x5F).
//!
//! The converters' analogue settings and the digital bus to the FPGA. Samples cross that
//! bus one 12-bit word at a time, I and Q alternating, with a frame-sync line marking
//! which is which. [`Interface`] sets that order, the frame-sync polarity and the clock
//! edges. They must match what the FPGA gateware expects, or I and Q arrive swapped or
//! misaligned.

use bnb::{BitEnum, bitfield, u2};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// AFE register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `RX_CTRL2`; see [`AdcReference`].
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

/// ADC reference gain adjust, as libusdr's YAML labels the codes.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum ReferenceGain {
    /// 1.50 V.
    Volts1_50,

    /// 1.75 V.
    Volts1_75,

    /// 1.00 V.
    Volts1_00,

    /// 1.25 V.
    Volts1_25,
}

/// ADC common-mode voltage.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum AdcCommonMode {
    /// 875 mV.
    Millivolts875,

    /// 960 mV.
    Millivolts960,

    /// 700 mV.
    Millivolts700,

    /// 790 mV.
    Millivolts790,
}

/// ADC reference buffer boost.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum BufferBoost {
    /// 1.0×.
    X1_0,

    /// 1.5×.
    X1_5,

    /// 2.0×.
    X2_0,

    /// 2.5×.
    X2_5,
}

/// The ADC's reference: gain, common mode and buffer boost. Datasheet `RX_CTRL2`, register
/// 0x59. Bit 0 is not in libusdr's map and has no field; [`AdcReference::from_raw`] keeps it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AdcReference {
    /// Reference gain adjust. `GAIN_ADJ`, bits 6:5.
    #[bits(5..=6)]
    gain: ReferenceGain,

    /// Common-mode adjust. `CM_ADJ`, bits 4:3.
    #[bits(3..=4)]
    common_mode: AdcCommonMode,

    /// Reference buffer boost. `BUF_BOOST`, bits 2:1.
    #[bits(1..=2)]
    buffer_boost: BufferBoost,
}
impl Register for AdcReference {
    type Map = Reg;
    const ADDR: Reg = Reg::AdcReference;
}

impl AdcReference {
    /// Lime's recommended setting, 0x29, from the LMS6002D FAQ v1.0r12, 5.27 (as libusdr
    /// writes it): 1.75 V reference gain, 960 mV common mode, 1.0× buffer boost, and the
    /// undocumented bit 0 set.
    pub(super) const LIME_RECOMMENDED: Self = Self::from_raw(0x29);
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
impl Register for Interface {
    type Map = Reg;
    const ADDR: Reg = Reg::Interface;
}

impl Interface {
    /// I then Q in both directions, both frame-sync polarity bits set, DAC on the negative
    /// edge, ADC on the positive edge (its reset is negative), nominal non-overlap: libusdr's
    /// power-up value, 0xB0 ("IQ, neg polarity").
    pub(super) const I_FIRST: Self = Self::new()
        .with_rx_frame_sync_polarity(true)
        .with_dac_negative_edge(true)
        .with_tx_frame_sync_polarity(true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lime_adc_reference_decodes_and_keeps_bit_0() {
        let reference = AdcReference::LIME_RECOMMENDED;
        assert_eq!(reference.gain(), ReferenceGain::Volts1_75);
        assert_eq!(reference.common_mode(), AdcCommonMode::Millivolts960);
        assert_eq!(reference.buffer_boost(), BufferBoost::X1_0);
        assert_eq!(
            u8::from(reference),
            0x29,
            "the FAQ byte, undocumented bit 0 included"
        );
    }
}
