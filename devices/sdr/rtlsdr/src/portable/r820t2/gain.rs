//! The IF VGA's gain steps (Table 1-3, R12, p. 8): code 0 is -12.0 dB, code 15 is
//! +40.5 dB, 3.5 dB a step. The LNA and mixer gain codes have no documented dB values
//! (`R820-GAP-04`); they stay raw field codes.

use super::field::{self, Field};
use super::setting::FieldValue;

/// Code 0's gain, in tenths of a dB. prov: R820-DS-34
pub const VGA_MIN_TENTH_DB: i16 = -120;

/// One code step, in tenths of a dB. prov: R820-DS-34
pub const VGA_STEP_TENTH_DB: i16 = 35;

/// A `VGA_CODE` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VgaGain(u8);

impl VgaGain {
    /// The code nearest `tenth_db`, clamped to the documented range. (A step is an odd
    /// number of tenths, so no input lies halfway.)
    #[must_use]
    pub fn nearest(tenth_db: i16) -> Self {
        let steps = (i32::from(tenth_db) - i32::from(VGA_MIN_TENTH_DB)
            + i32::from(VGA_STEP_TENTH_DB) / 2)
            .div_euclid(i32::from(VGA_STEP_TENTH_DB));
        // Clamped to the field, so the conversion below cannot fail.
        let code = steps.clamp(0, i32::from(field::VGA_CODE.max()));
        Self(u8::try_from(code).unwrap_or_default())
    }

    /// The gain, in tenths of a dB.
    #[must_use]
    pub fn tenth_db(self) -> i16 {
        VGA_MIN_TENTH_DB + VGA_STEP_TENTH_DB * i16::from(self.0)
    }
}

impl FieldValue for VgaGain {
    const FIELD: Field = field::VGA_CODE;

    fn bits(self) -> u8 {
        self.0
    }

    fn from_bits(bits: u8) -> Option<Self> {
        (bits <= Self::FIELD.max()).then_some(Self(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gain of `code`.
    fn tenth_db(code: u8) -> i16 {
        VgaGain::from_bits(code).unwrap().tenth_db()
    }

    #[test]
    fn the_codes_span_the_documented_endpoints() {
        // R820-DS-34 (p. 8): 0000 = -12.0 dB, 1111 = +40.5 dB.
        assert_eq!(tenth_db(0b0000), -120);
        assert_eq!(tenth_db(0b1111), 405);
        assert_eq!(VgaGain::from_bits(16), None);
    }

    #[test]
    fn nearest_picks_the_closest_code_and_clamps() {
        assert_eq!(VgaGain::nearest(-120).bits(), 0);
        assert_eq!(VgaGain::nearest(405).bits(), 15);
        assert_eq!(
            VgaGain::nearest(30).tenth_db(),
            20,
            "3.0 dB is nearer 2.0 than 5.5"
        );
        assert_eq!(
            VgaGain::nearest(-103).tenth_db(),
            -120,
            "1.7 dB up is under half a step"
        );
        assert_eq!(
            VgaGain::nearest(-102).tenth_db(),
            -85,
            "1.8 dB up is over half a step"
        );
        assert_eq!(VgaGain::nearest(-500).bits(), 0);
        assert_eq!(VgaGain::nearest(1000).bits(), 15);
    }
}
