//! The register fields Table 1-2 (p. 5) and Table 1-3 (pp. 6-11) define.
//!
//! Names follow the document. Where its two tables disagree on a field's name, width or
//! bits, the row in `PROVENANCE.md` records the conflict and which reading is used here.
//! Bits Table 1-2 shows as fixed 0/1 are not fields: whether they are required values or
//! defaults is not stated (`R820-GAP-08`).

use super::registers::RegisterError;

/// A run of bits in one register.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    /// The register's address.
    register: u8,

    /// The lowest bit.
    shift: u8,

    /// How many bits.
    width: u8,
}

impl Field {
    /// Bits `shift..shift + width` of `register`.
    const fn new(register: u8, shift: u8, width: u8) -> Self {
        assert!(
            width > 0 && shift + width <= 8,
            "a field lies within one byte"
        );
        assert!(register < 32, "the chip has registers R0-R31");
        Self {
            register,
            shift,
            width,
        }
    }

    /// The register's address.
    #[must_use]
    pub const fn register(self) -> u8 {
        self.register
    }

    /// The largest value the field holds.
    #[must_use]
    pub const fn max(self) -> u8 {
        u8::MAX >> (8 - self.width)
    }

    /// The field's bits within its register.
    #[must_use]
    pub const fn mask(self) -> u8 {
        self.max() << self.shift
    }

    /// The field's value in `byte`.
    #[must_use]
    pub const fn extract(self, byte: u8) -> u8 {
        (byte & self.mask()) >> self.shift
    }

    /// `byte` with the field set to `value`.
    ///
    /// # Errors
    ///
    /// [`RegisterError::TooWide`] if `value` does not fit.
    pub const fn insert(self, byte: u8, value: u8) -> Result<u8, RegisterError> {
        if value > self.max() {
            return Err(RegisterError::TooWide { field: self, value });
        }
        Ok(byte & !self.mask() | value << self.shift)
    }
}

/// R2 `VCO_INDICATOR[6:0]` (read-only). prov: R820-DS-07
pub const VCO_INDICATOR: Field = Field::new(0x02, 0, 7);
/// R3 `RF_INDICATOR[7:0]` (read-only). prov: R820-DS-07
pub const RF_INDICATOR: Field = Field::new(0x03, 0, 8);

/// R5\[7\] loop-through power, 0 on. prov: R820-DS-10
pub const PWD_LT: Field = Field::new(0x05, 7, 1);
/// R5\[5\] LNA1 power, 0 on. prov: R820-DS-10
pub const PWD_LNA1: Field = Field::new(0x05, 5, 1);
/// R5\[4\] LNA gain mode ([`LnaGainMode`](super::setting::LnaGainMode)). prov: R820-DS-10
pub const LNA_GAIN_MODE: Field = Field::new(0x05, 4, 1);
/// R5\[3:0\] LNA manual gain, 0 min to 15 max. prov: R820-DS-10
pub const LNA_GAIN: Field = Field::new(0x05, 0, 4);

/// R6\[7\] power detector 1, 0 on. prov: R820-DS-11
pub const PWD_PDET1: Field = Field::new(0x06, 7, 1);
/// R6\[6\] power detector 3, 1 on. prov: R820-DS-11
pub const PWD_PDET3: Field = Field::new(0x06, 6, 1);
/// R6\[5\] filter gain, 1 adds 3 dB. prov: R820-DS-11
pub const FILT_3DB: Field = Field::new(0x06, 5, 1);
/// R6\[2:0\] LNA power, 0 max to 7 min. prov: R820-DS-11
pub const PW_LNA: Field = Field::new(0x06, 0, 3);

/// R7\[6\] mixer power, 1 on. prov: R820-DS-12
pub const PWD_MIX: Field = Field::new(0x07, 6, 1);
/// R7\[5\] mixer current, 0 max, 1 normal. prov: R820-DS-12
pub const PW0_MIX: Field = Field::new(0x07, 5, 1);
/// R7\[4\] mixer gain mode ([`MixerGainMode`](super::setting::MixerGainMode)). prov: R820-DS-12
pub const MIXGAIN_MODE: Field = Field::new(0x07, 4, 1);
/// R7\[3:0\] mixer manual gain, 0 min to 15 max. prov: R820-DS-12
pub const MIX_GAIN: Field = Field::new(0x07, 0, 4);

/// R8\[7\] mixer buffer power, 1 on. prov: R820-DS-13
pub const PWD_AMP: Field = Field::new(0x08, 7, 1);
/// R8\[6\] mixer buffer current, 0 high, 1 low. prov: R820-DS-13
pub const PW0_AMP: Field = Field::new(0x08, 6, 1);
/// R8\[5:0\] image gain adjustment, 0 min to 63 max. prov: R820-DS-13
pub const IMR_G: Field = Field::new(0x08, 0, 6);

/// R9\[7\] IF filter power, 0 on. prov: R820-DS-14
pub const PWD_IFFILT: Field = Field::new(0x09, 7, 1);
/// R9\[6\] IF filter current, 0 high, 1 low. prov: R820-DS-14
pub const PW1_IFFILT: Field = Field::new(0x09, 6, 1);
/// R9\[5:0\] image phase adjustment, 0 min to 63 max. prov: R820-DS-14
pub const IMR_P: Field = Field::new(0x09, 0, 6);

/// R10\[7\] channel filter power, 1 on. prov: R820-DS-15
pub const PWD_FILT: Field = Field::new(0x0A, 7, 1);
/// R10\[6:5\] filter power, 0 highest to 3 lowest. prov: R820-DS-15
pub const PW_FILT: Field = Field::new(0x0A, 5, 2);
/// R10\[3:0\] filter bandwidth fine tune, 0 widest to 15 narrowest. prov: R820-DS-15
pub const FILT_CODE: Field = Field::new(0x0A, 0, 4);

/// R11\[6:5\] filter bandwidth coarse tune ([`FilterBandwidth`](super::setting::FilterBandwidth)).
/// prov: R820-DS-16
pub const FILT_BW: Field = Field::new(0x0B, 5, 2);
/// R11\[3:0\] high-pass corner, 0 highest to 15 lowest. prov: R820-DS-16
pub const HPF: Field = Field::new(0x0B, 0, 4);

/// R12\[6\] IF VGA power, 1 on. prov: R820-DS-17
pub const PWD_VGA: Field = Field::new(0x0C, 6, 1);
/// R12\[4\] IF VGA gain source ([`VgaMode`](super::setting::VgaMode)). prov: R820-DS-17
pub const VGA_MODE: Field = Field::new(0x0C, 4, 1);
/// R12\[3:0\] IF VGA manual gain ([`VgaGain`](super::gain::VgaGain)). prov: R820-DS-17
pub const VGA_CODE: Field = Field::new(0x0C, 0, 4);

/// R13\[7:4\] LNA AGC detector high threshold, 0 = 0.34 V to 15 = 1.94 V. prov: R820-DS-18
pub const LNA_VTH_H: Field = Field::new(0x0D, 4, 4);
/// R13\[3:0\] LNA AGC detector low threshold, 0 = 0.34 V to 15 = 1.94 V. prov: R820-DS-18
pub const LNA_VTH_L: Field = Field::new(0x0D, 0, 4);

/// R14\[7:4\] mixer AGC detector high threshold, 0 = 0.34 V to 15 = 1.94 V. prov: R820-DS-19
pub const MIX_VTH_H: Field = Field::new(0x0E, 4, 4);
/// R14\[3:0\] mixer AGC detector low threshold, 0 = 0.34 V to 15 = 1.94 V. prov: R820-DS-19
pub const MIX_VTH_L: Field = Field::new(0x0E, 0, 4);

/// R15\[4\] clock output pin, 0 on. prov: R820-DS-20
pub const CLK_OUT_ENB: Field = Field::new(0x0F, 4, 1);
/// R15\[1\] internal AGC clock, 0 on. prov: R820-DS-20
pub const CLK_AGC_ENB: Field = Field::new(0x0F, 1, 1);

/// R16\[7:5\] PLL-to-mixer divider ([`MixerDivider`](super::setting::MixerDivider)).
/// prov: R820-DS-21
pub const SEL_DIV: Field = Field::new(0x10, 5, 3);
/// R16\[4\] reference divider ([`RefDivider`](super::setting::RefDivider)). prov: R820-DS-21
pub const REF_DIV2: Field = Field::new(0x10, 4, 1);
/// R16\[1:0\] internal crystal load ([`XtalCap`](super::setting::XtalCap)). prov: R820-DS-21
pub const CAPX: Field = Field::new(0x10, 0, 2);

/// R17\[7:6\] PLL analog LDO ([`AnalogLdo`](super::setting::AnalogLdo)). prov: R820-DS-22
pub const PW_LDO_A: Field = Field::new(0x11, 6, 2);

/// R18\[3\] `PW_SDM`; Table 1-2 names it, Table 1-3 does not describe it. prov: R820-DS-23
pub const PW_SDM: Field = Field::new(0x12, 3, 1);

/// R20\[7:6\] PLL integer divider input `SI2C` ([`PllDivider`](super::pll::PllDivider)).
/// prov: R820-DS-24
pub const SI2C: Field = Field::new(0x14, 6, 2);
/// R20\[5:0\] PLL integer divider input `NI2C`. prov: R820-DS-24
pub const NI2C: Field = Field::new(0x14, 0, 6);
/// R21 `SDM_IN[8:1]`, the fractional input's low byte. prov: R820-DS-25
pub const SDM_IN_LOW: Field = Field::new(0x15, 0, 8);
/// R22 `SDM_IN[16:9]`, the fractional input's high byte. prov: R820-DS-25
pub const SDM_IN_HIGH: Field = Field::new(0x16, 0, 8);

/// R23\[7:6\] PLL digital LDO ([`DigitalLdo`](super::setting::DigitalLdo)). prov: R820-DS-27
pub const PW_LDO_D: Field = Field::new(0x17, 6, 2);
/// R23\[3\] open drain, 0 high-Z, 1 low-Z. prov: R820-DS-27
pub const OPEN_D: Field = Field::new(0x17, 3, 1);

/// R25\[7\] RF filter power, 1 on. prov: R820-DS-28
pub const PWD_RFFILT: Field = Field::new(0x19, 7, 1);
/// R25\[4\] AGC pin switch, 0 `agc_in`, 1 `agc_in2`. prov: R820-DS-28
pub const SW_AGC: Field = Field::new(0x19, 4, 1);

/// R26\[7:6\] tracking filter switch ([`RfMux`](super::setting::RfMux)). prov: R820-DS-29
pub const RFMUX: Field = Field::new(0x1A, 6, 2);
/// R26\[3:2\] PLL auto-tune clock ([`PllAutoClock`](super::setting::PllAutoClock)).
/// prov: R820-DS-29
pub const PLL_AUTO_CLK: Field = Field::new(0x1A, 2, 2);
/// R26\[1:0\] RF filter band ([`RfFilterBand`](super::setting::RfFilterBand)). prov: R820-DS-29
pub const RFFILT: Field = Field::new(0x1A, 0, 2);

/// R27\[7:4\] tracking filter notch corner, 0 highest to 15 lowest. prov: R820-DS-30
pub const TF_NCH: Field = Field::new(0x1B, 4, 4);
/// R27\[3:0\] tracking filter low-pass corner, 0 highest to 15 lowest. prov: R820-DS-30
pub const TF_LP: Field = Field::new(0x1B, 0, 4);

/// R28\[7:4\] power detector 3 take-off point, 0 highest to 15 lowest. prov: R820-DS-31
pub const PDET3_GAIN: Field = Field::new(0x1C, 4, 4);

/// R29\[5:3\] power detector 1 take-off point, 0 highest. prov: R820-DS-32
pub const PDET1_GAIN: Field = Field::new(0x1D, 3, 3);
/// R29\[2:0\] power detector 2 take-off point, 0 lowest to 7 highest. prov: R820-DS-32
pub const PDET2_GAIN: Field = Field::new(0x1D, 0, 3);

/// R30\[6\] filter extension under weak signal, 1 enabled. prov: R820-DS-33
pub const FILTER_EXT: Field = Field::new(0x1E, 6, 1);
/// R30\[5:0\] power detector timing, 0 min to 63 max. prov: R820-DS-33
pub const PDET_CLK: Field = Field::new(0x1E, 0, 6);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_extract_round_trip_without_touching_other_bits() {
        let byte = SEL_DIV.insert(0b0001_1111, 0b101).unwrap();
        assert_eq!(byte, 0b1011_1111);
        assert_eq!(SEL_DIV.extract(byte), 0b101);
        assert!(SEL_DIV.insert(0, 8).is_err(), "SEL_DIV is three bits");
    }

    #[test]
    fn no_two_fields_of_a_register_overlap() {
        let fields = [
            VCO_INDICATOR,
            RF_INDICATOR,
            PWD_LT,
            PWD_LNA1,
            LNA_GAIN_MODE,
            LNA_GAIN,
            PWD_PDET1,
            PWD_PDET3,
            FILT_3DB,
            PW_LNA,
            PWD_MIX,
            PW0_MIX,
            MIXGAIN_MODE,
            MIX_GAIN,
            PWD_AMP,
            PW0_AMP,
            IMR_G,
            PWD_IFFILT,
            PW1_IFFILT,
            IMR_P,
            PWD_FILT,
            PW_FILT,
            FILT_CODE,
            FILT_BW,
            HPF,
            PWD_VGA,
            VGA_MODE,
            VGA_CODE,
            LNA_VTH_H,
            LNA_VTH_L,
            MIX_VTH_H,
            MIX_VTH_L,
            CLK_OUT_ENB,
            CLK_AGC_ENB,
            SEL_DIV,
            REF_DIV2,
            CAPX,
            PW_LDO_A,
            PW_SDM,
            SI2C,
            NI2C,
            SDM_IN_LOW,
            SDM_IN_HIGH,
            PW_LDO_D,
            OPEN_D,
            PWD_RFFILT,
            SW_AGC,
            RFMUX,
            PLL_AUTO_CLK,
            RFFILT,
            TF_NCH,
            TF_LP,
            PDET3_GAIN,
            PDET1_GAIN,
            PDET2_GAIN,
            FILTER_EXT,
            PDET_CLK,
        ];
        let mut used = [0_u8; 32];
        for field in fields {
            let register = usize::from(field.register());
            assert_eq!(used[register] & field.mask(), 0, "{field:?} overlaps");
            used[register] |= field.mask();
        }
    }
}
