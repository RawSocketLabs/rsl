//! The enumerated settings Table 1-3 defines, one type per field. Codes the document
//! leaves undefined decode to `None`; nothing here assigns them a meaning.

use super::field::{self, Field};

/// A field's typed value.
pub trait FieldValue: Copy + Sized {
    /// The field it occupies.
    const FIELD: Field;

    /// The field's bits for this value.
    fn bits(self) -> u8;

    /// The value the field's bits name, if the document defines one.
    fn from_bits(bits: u8) -> Option<Self>;
}

/// Declares a field-valued enum: each variant's bits, and the field.
macro_rules! setting {
    ($(#[$meta:meta])* $name:ident in $field:path { $($(#[$vmeta:meta])* $variant:ident = $bits:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
        }

        impl FieldValue for $name {
            const FIELD: Field = $field;

            fn bits(self) -> u8 {
                match self {
                    $(Self::$variant => $bits,)+
                }
            }

            fn from_bits(bits: u8) -> Option<Self> {
                match bits {
                    $($bits => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

setting! {
    /// R5\[4\] `LNA_GAIN_MODE`. prov: R820-DS-10
    LnaGainMode in field::LNA_GAIN_MODE {
        /// The LNA's AGC sets its gain.
        Auto = 0,
        /// `LNA_GAIN` sets it.
        Manual = 1,
    }
}

setting! {
    /// R7\[4\] `MIXGAIN_MODE`; note the polarity is the opposite of the LNA's.
    /// prov: R820-DS-12
    MixerGainMode in field::MIXGAIN_MODE {
        /// `MIX_GAIN` sets the gain.
        Manual = 0,
        /// The mixer's AGC sets it.
        Auto = 1,
    }
}

setting! {
    /// R11\[6:5\] `FILT_BW`, the channel filter's coarse bandwidth. Both middle codes are
    /// kept: the document gives them one meaning but does not say they are equal.
    /// prov: R820-DS-16
    FilterBandwidth in field::FILT_BW {
        /// `00`.
        Widest = 0b00,
        /// `01`.
        Middle01 = 0b01,
        /// `10`.
        Middle10 = 0b10,
        /// `11`.
        Narrowest = 0b11,
    }
}

setting! {
    /// R12\[4\] `VGA_MODE`: what sets the IF VGA's gain. prov: R820-DS-17
    VgaMode in field::VGA_MODE {
        /// `VGA_CODE`.
        Code = 0,
        /// The `vagc` pin.
        Pin = 1,
    }
}

setting! {
    /// R16\[7:5\] `SEL_DIV`: mixer input = VCO output / [`ratio`](Self::ratio). Codes
    /// `100`-`111` are undefined (`R820-GAP-03`). prov: R820-DS-21
    MixerDivider in field::SEL_DIV {
        /// `000`: VCO / 2.
        Div2 = 0b000,
        /// `001`: VCO / 4.
        Div4 = 0b001,
        /// `010`: VCO / 8.
        Div8 = 0b010,
        /// `011`: VCO / 1.
        Div1 = 0b011,
    }
}

impl MixerDivider {
    /// What the VCO output is divided by.
    #[must_use]
    pub fn ratio(self) -> u32 {
        match self {
            Self::Div1 => 1,
            Self::Div2 => 2,
            Self::Div4 => 4,
            Self::Div8 => 8,
        }
    }
}

setting! {
    /// R16\[4\] `REF_DIV2`: the PLL reference. prov: R820-DS-21
    RefDivider in field::REF_DIV2 {
        /// `fref = xtal`.
        Xtal = 0,
        /// `fref = xtal / 2`, which the document advises above 24 MHz.
        XtalHalf = 1,
    }
}

/// The crystal frequency above which the document advises halving it. prov: R820-DS-35
pub const HALVE_XTAL_ABOVE_HZ: u32 = 24_000_000;

impl RefDivider {
    /// The divider the document advises for a crystal of `xtal_hz`.
    #[must_use]
    pub fn advised(xtal_hz: u32) -> Self {
        if xtal_hz > HALVE_XTAL_ABOVE_HZ {
            Self::XtalHalf
        } else {
            Self::Xtal
        }
    }

    /// The PLL reference for a crystal of `xtal_hz`; odd frequencies halve to the floor.
    #[must_use]
    pub fn reference_hz(self, xtal_hz: u32) -> u32 {
        match self {
            Self::Xtal => xtal_hz,
            Self::XtalHalf => xtal_hz / 2,
        }
    }
}

setting! {
    /// R16\[1:0\] `CAPX`: the internal crystal load. prov: R820-DS-21
    XtalCap in field::CAPX {
        /// No capacitor.
        NoCap = 0b00,
        /// 10 pF.
        Pf10 = 0b01,
        /// 20 pF.
        Pf20 = 0b10,
        /// 30 pF.
        Pf30 = 0b11,
    }
}

setting! {
    /// R17\[7:6\] `PW_LDO_A`: the PLL analog regulator. prov: R820-DS-22
    AnalogLdo in field::PW_LDO_A {
        /// Off.
        Off = 0b00,
        /// 2.1 V.
        V2_1 = 0b01,
        /// 2.0 V.
        V2_0 = 0b10,
        /// 1.9 V.
        V1_9 = 0b11,
    }
}

setting! {
    /// R23\[7:6\] `PW_LDO_D`: the PLL digital regulator's supply. prov: R820-DS-27
    DigitalLdo in field::PW_LDO_D {
        /// 1.8 V, 8 mA.
        V1_8Ma8 = 0b00,
        /// 1.8 V, 4 mA.
        V1_8Ma4 = 0b01,
        /// 2.0 V, 8 mA.
        V2_0Ma8 = 0b10,
        /// Off.
        Off = 0b11,
    }
}

setting! {
    /// R26\[7:6\] `RFMUX`: the tracking filter switch. Codes `10` and `11` are undefined
    /// (`R820-GAP-06`). prov: R820-DS-29
    RfMux in field::RFMUX {
        /// Tracking filter on.
        TrackingFilter = 0b00,
        /// Bypassed.
        Bypass = 0b01,
    }
}

setting! {
    /// R26\[3:2\] `PLL_AUTO_CLK`: the PLL auto-tune clock. Code `11` is undefined
    /// (`R820-GAP-06`). prov: R820-DS-29
    PllAutoClock in field::PLL_AUTO_CLK {
        /// 128 kHz.
        Khz128 = 0b00,
        /// 32 kHz.
        Khz32 = 0b01,
        /// 8 kHz.
        Khz8 = 0b10,
    }
}

impl PllAutoClock {
    /// The clock rate.
    #[must_use]
    pub fn hz(self) -> u32 {
        match self {
            Self::Khz128 => 128_000,
            Self::Khz32 => 32_000,
            Self::Khz8 => 8_000,
        }
    }
}

setting! {
    /// R26\[1:0\] `RFFILT`: the RF filter band. Code `11` and the bands' edges are
    /// undefined (`R820-GAP-06`). prov: R820-DS-29
    RfFilterBand in field::RFFILT {
        /// The highest band.
        High = 0b00,
        /// The middle band.
        Medium = 0b01,
        /// The low band.
        Low = 0b10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::r820t2::Registers;

    #[test]
    fn the_mixer_divider_codes_follow_table_1_3() {
        // R820-DS-21 (p. 9): 000 /2, 001 /4, 010 /8, 011 /1.
        let ratios: Vec<_> = (0..8)
            .map(|bits| MixerDivider::from_bits(bits).map(MixerDivider::ratio))
            .collect();
        assert_eq!(
            ratios,
            [Some(2), Some(4), Some(8), Some(1), None, None, None, None]
        );
    }

    #[test]
    fn the_gain_mode_bits_have_opposite_polarities() {
        // R820-DS-10 / R820-DS-12 (p. 6): LNA 0 auto, mixer 0 manual.
        assert_eq!(LnaGainMode::from_bits(0), Some(LnaGainMode::Auto));
        assert_eq!(MixerGainMode::from_bits(0), Some(MixerGainMode::Manual));
    }

    #[test]
    fn the_reference_is_halved_only_above_24_mhz() {
        // R820-DS-35 (p. 9).
        assert_eq!(RefDivider::advised(24_000_000), RefDivider::Xtal);
        assert_eq!(RefDivider::advised(24_000_001), RefDivider::XtalHalf);
        assert_eq!(RefDivider::XtalHalf.reference_hz(30_000_001), 15_000_000);
        assert_eq!(RefDivider::Xtal.reference_hz(16_000_000), 16_000_000);
    }

    #[test]
    fn undefined_codes_decode_to_nothing() {
        assert_eq!(RfMux::from_bits(0b10), None);
        assert_eq!(PllAutoClock::from_bits(0b11), None);
        assert_eq!(RfFilterBand::from_bits(0b11), None);
    }

    #[test]
    fn a_setting_lands_in_its_field_and_reads_back() {
        let mut registers = Registers::default();
        registers.put(MixerDivider::Div8);
        registers.put(XtalCap::Pf20);
        assert_eq!(registers.byte(0x10), Some(0b0100_0010));
        assert_eq!(registers.take(), Some(MixerDivider::Div8));
        assert_eq!(registers.take(), Some(XtalCap::Pf20));
        assert_eq!(PllAutoClock::Khz32.hz(), 32_000);
    }
}
