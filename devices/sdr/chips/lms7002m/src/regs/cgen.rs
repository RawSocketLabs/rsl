//! The clock generator (`CGEN` page): the PLL that clocks the converters and the digital
//! interface, its dividers, VCO and tuning comparators.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics, and docs say what libusdr's `lms7002m_cgen_tune` writes.

use bnb::{BitEnum, bitfield, u2, u3, u4, u5, u10};

use crate::reg::Reg;
use crate::register::Register;

/// Power-downs, resets and enables. `CGEN_ENABLE` (0x0086).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgenEnable {
    /// `SPDUP_VCO`, bit 15.
    #[bits(15..=15)]
    vco_speed_up: bool,

    /// `RESET_N`, bit 14; libusdr sets it while tuning and clears it to disable.
    #[bits(14..=14)]
    running: bool,

    /// `EN_ADCCLKH_CLKGN`, bit 11.
    #[bits(11..=11)]
    adc_clock_high: bool,

    /// `EN_COARSE_CKLGEN`, bit 10.
    #[bits(10..=10)]
    coarse: bool,

    /// `EN_INTONLY_SDM_CGEN`, bit 9.
    #[bits(9..=9)]
    integer_only: bool,

    /// `EN_SDM_CLK_CGEN`, bit 8.
    #[bits(8..=8)]
    sdm_clock: bool,

    /// `PD_CP_CGEN`, bit 6.
    #[bits(6..=6)]
    charge_pump_powered_down: bool,

    /// `PD_FDIV_FB_CGEN`, bit 5.
    #[bits(5..=5)]
    feedback_divider_powered_down: bool,

    /// `PD_FDIV_O_CGEN`, bit 4.
    #[bits(4..=4)]
    output_divider_powered_down: bool,

    /// `PD_SDM_CGEN`, bit 3.
    #[bits(3..=3)]
    sdm_powered_down: bool,

    /// `PD_VCO_CGEN`, bit 2.
    #[bits(2..=2)]
    vco_powered_down: bool,

    /// `PD_VCO_COMP_CGEN`, bit 1.
    #[bits(1..=1)]
    comparator_powered_down: bool,

    /// The generator's global enable. `EN_G_CGEN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for CgenEnable {
    const ADDR: Reg = Reg::CgenEnable;
}

/// The feedback divider's fraction, bits 15:0 of 20. `CGEN_0x0087` (`FRAC_SDM_CGEN`).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgenFracLow {
    /// `FRAC_SDM_CGEN[15:0]`.
    #[bits(0..=15)]
    fraction: u16,
}

impl Register for CgenFracLow {
    const ADDR: Reg = Reg::CgenFracLow;
}

/// The feedback divider's integer part and the fraction's top bits. `CGEN_0x0088`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgenIntFrac {
    /// The integer divider minus one. `INT_SDM_CGEN`, bits 13:4.
    #[bits(4..=13)]
    integer: u10,

    /// `FRAC_SDM_CGEN[19:16]`, bits 3:0.
    #[bits(0..=3)]
    fraction_high: u4,
}

impl Register for CgenIntFrac {
    const ADDR: Reg = Reg::CgenIntFrac;
}

/// Output dividers and test modes. `CGEN_0x0089`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgenDivider {
    /// `REV_SDMCLK_CGEN`, bit 15.
    #[bits(15..=15)]
    reverse_sdm_clock: bool,

    /// `SEL_SDMCLK_CGEN`, bit 14: 0 the divided clock, 1 the reference.
    #[bits(14..=14)]
    sdm_clock_from_reference: bool,

    /// `SX_DITHER_EN_CGEN`, bit 13.
    #[bits(13..=13)]
    dither: bool,

    /// The DAC clock's divider order relative to the ADC's (libusdr's `txdiv_ord`: 0, 1,
    /// 2 or 3 for /1, /2, /4, /8). `CLKH_OV_CLKL_CGEN`, bits 12:11.
    #[bits(11..=12)]
    clock_h_over_l: u2,

    /// The output divider: half the division minus one. `DIV_OUTCH_CGEN`, bits 10:3.
    #[bits(3..=10)]
    output_divider: u8,

    /// `TST_CGEN`, bits 2:0.
    #[bits(0..=2)]
    test: u3,
}

impl Register for CgenDivider {
    const ADDR: Reg = Reg::CgenDivider;
}

/// The VCO's bias and capacitor bank. `CGEN_0x008B`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CgenVco {
    /// `ICT_VCO_CGEN`, bits 13:9; libusdr's trims write 15.
    #[bits(9..=13)]
    bias: u5,

    /// The capacitor bank code. `CSW_VCO_CGEN`, bits 8:1.
    #[bits(1..=8)]
    cap: u8,

    /// `COARSE_START_CGEN`, bit 0.
    #[bits(0..=0)]
    coarse_start: bool,
}

impl Register for CgenVco {
    const ADDR: Reg = Reg::CgenVco;
}

/// The VCO tuning comparators, read after a cap code is written. `CGEN_0x008C`
/// (`VCO_CMPHO`, bit 13; `VCO_CMPLO`, bit 12) and `SXX_0x0123`, which places them alike.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum VcoReading {
    /// Neither comparator: the VCO runs too fast for the code (`LMS7002M_VCO_LOW`).
    Low,

    /// Only `VCO_CMPLO`: libusdr's `LMS7002M_VCO_FAIL`, which ends a search with `-EIO`.
    Fail,

    /// Only `VCO_CMPHO`: in range (`LMS7002M_VCO_OK`).
    Ok,

    /// Both: the VCO runs too slow for the code (`LMS7002M_VCO_HIGH`).
    High,
}

/// A VCO comparator register as read: `CGEN_0x008C` or `SXX_0x0123`, which share the
/// layout. Only the reading is interpreted; libusdr reads nothing else there.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VcoComparators {
    /// `VCO_CMPHO` and `VCO_CMPLO`, bits 13:12.
    #[bits(12..=13)]
    reading: VcoReading,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_follow_libusdrs_comparator_encoding() {
        // (VCO_CMPHO << 1) | VCO_CMPLO, as lms7002m_cgen_trim_vco returns it.
        let reading = |raw| VcoComparators::from_raw(raw).reading();
        assert_eq!(reading(0x0000), VcoReading::Low);
        assert_eq!(reading(0x1000), VcoReading::Fail);
        assert_eq!(reading(0x2fff), VcoReading::Ok);
        assert_eq!(reading(0xf000), VcoReading::High);
    }

    #[test]
    fn the_cap_code_sits_above_the_coarse_start_bit() {
        // MAKE_LMS7002M_CGEN_0x008B(15, 0x80, 0).
        let vco = CgenVco::new().with_bias(u5::new(15)).with_cap(0x80);
        assert_eq!(vco.to_raw(), 15 << 9 | 0x80 << 1);
    }
}
