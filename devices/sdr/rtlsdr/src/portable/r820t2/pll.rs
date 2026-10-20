//! The PLL divider number (Table 1-3, R20-R22, p. 9).
//!
//! `Nint = 4·NI2C + SI2C + 13`, `Nfra = SDM_IN / 2^16` (`SDM_IN[16]` weighs 2^-1 down to
//! `SDM_IN[1]` at 2^-16) and `Ndiv = (Nint + Nfra)·2`. The document does not state how
//! `Ndiv` relates the VCO to the reference, nor the VCO's range, so no LO plan is derived
//! here (`R820-GAP-01`, `R820-GAP-02`).

use super::field;
use super::registers::{RegisterError, Registers};

/// `Nint`'s weight on `NI2C`. prov: R820-DS-24
pub const NI2C_WEIGHT: u32 = 4;

/// `Nint`'s offset. prov: R820-DS-24
pub const NINT_OFFSET: u32 = 13;

/// `Ndiv`'s factor on `Nint + Nfra`. prov: R820-DS-26
pub const NDIV_FACTOR: u32 = 2;

/// Fractional bits of `SDM_IN`: `Nfra = SDM_IN / 2^SDM_BITS`. prov: R820-DS-25
pub const SDM_BITS: u32 = 16;

/// The smallest `Nint`: both inputs zero. prov: R820-DS-24
pub const NINT_MIN: u32 = NINT_OFFSET;

/// The largest `Nint`: both inputs at their fields' maxima. prov: R820-DS-24
// `u32::from` is not const; the casts widen u8 losslessly.
pub const NINT_MAX: u32 =
    NI2C_WEIGHT * field::NI2C.max() as u32 + field::SI2C.max() as u32 + NINT_OFFSET;

/// The PLL divider inputs `SI2C`, `NI2C` and `SDM_IN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PllDivider {
    /// `SI2C[1:0]`.
    si2c: u8,

    /// `NI2C[5:0]`.
    ni2c: u8,

    /// `SDM_IN[16:1]`, `SDM_IN[16]` its most significant bit.
    sdm: u16,
}

impl PllDivider {
    /// The divider with these inputs.
    ///
    /// # Errors
    ///
    /// [`RegisterError::TooWide`] if `si2c` or `ni2c` does not fit its field.
    pub fn new(si2c: u8, ni2c: u8, sdm: u16) -> Result<Self, RegisterError> {
        field::SI2C.insert(0, si2c)?;
        field::NI2C.insert(0, ni2c)?;
        Ok(Self { si2c, ni2c, sdm })
    }

    /// The divider whose `Ndiv` is `ndiv_q16 / 2^16`, if one is.
    #[must_use]
    pub fn from_ndiv_q16(ndiv_q16: u32) -> Option<Self> {
        if ndiv_q16 % NDIV_FACTOR != 0 {
            return None;
        }
        let n = ndiv_q16 / NDIV_FACTOR;
        let nint = n >> SDM_BITS;
        if !(NINT_MIN..=NINT_MAX).contains(&nint) {
            return None;
        }
        let integer = nint - NINT_OFFSET;
        Some(Self {
            si2c: u8::try_from(integer % NI2C_WEIGHT).ok()?,
            ni2c: u8::try_from(integer / NI2C_WEIGHT).ok()?,
            sdm: u16::try_from(n & ((1 << SDM_BITS) - 1)).ok()?,
        })
    }

    /// `Nint`.
    #[must_use]
    pub fn nint(self) -> u32 {
        NI2C_WEIGHT * u32::from(self.ni2c) + u32::from(self.si2c) + NINT_OFFSET
    }

    /// `Ndiv` in units of 2^-16: exact, since `Nfra` has 16 fractional bits.
    #[must_use]
    pub fn ndiv_q16(self) -> u32 {
        NDIV_FACTOR * ((self.nint() << SDM_BITS) + u32::from(self.sdm))
    }

    /// Writes the inputs into R20-R22.
    ///
    /// # Panics
    ///
    /// Never: the inputs were checked against their fields at construction.
    pub fn write(self, registers: &mut Registers) {
        let [high, low] = self.sdm.to_be_bytes();
        for (field, value) in [
            (field::SI2C, self.si2c),
            (field::NI2C, self.ni2c),
            (field::SDM_IN_LOW, low),
            (field::SDM_IN_HIGH, high),
        ] {
            registers
                .set(field, value)
                .expect("invariant: checked at construction");
        }
    }

    /// The inputs R20-R22 hold.
    #[must_use]
    pub fn read(registers: &Registers) -> Self {
        Self {
            si2c: registers.get(field::SI2C),
            ni2c: registers.get(field::NI2C),
            sdm: u16::from_be_bytes([
                registers.get(field::SDM_IN_HIGH),
                registers.get(field::SDM_IN_LOW),
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 in units of 2^-16.
    const ONE: u32 = 1 << 16;

    #[test]
    fn ndiv_follows_the_documented_formula() {
        // R820-DS-24..26 (p. 9): Nint = 4·NI2C + SI2C + 13, Ndiv = (Nint + Nfra)·2.
        let ndiv = |si2c, ni2c, sdm| PllDivider::new(si2c, ni2c, sdm).unwrap().ndiv_q16();
        assert_eq!(ndiv(0, 0, 0), 26 * ONE, "Nint 13, no fraction");
        assert_eq!(ndiv(3, 63, 0), 2 * 268 * ONE, "the largest Nint");
        assert_eq!(ndiv(1, 2, 0), 2 * 22 * ONE);
        assert_eq!(ndiv(0, 0, 0x8000), 27 * ONE, "SDM_IN[16] weighs 2^-1");
        assert_eq!(ndiv(0, 0, 0x0001), 26 * ONE + 2, "SDM_IN[1] weighs 2^-16");
    }

    #[test]
    fn every_reachable_ndiv_decodes_to_the_inputs_that_make_it() {
        for nint in NINT_MIN..=NINT_MAX {
            for sdm in [0, 1, 0x8000, 0xFFFF] {
                let ndiv = NDIV_FACTOR * ((nint << SDM_BITS) + sdm);
                let divider = PllDivider::from_ndiv_q16(ndiv).expect("reachable");
                assert_eq!(divider.nint(), nint);
                assert_eq!(divider.ndiv_q16(), ndiv);
            }
        }
    }

    #[test]
    fn unreachable_ndivs_are_refused() {
        assert_eq!(PllDivider::from_ndiv_q16(26 * ONE + 1), None, "odd");
        assert_eq!(
            PllDivider::from_ndiv_q16(26 * ONE - 2),
            None,
            "below Nint 13"
        );
        assert_eq!(
            PllDivider::from_ndiv_q16(2 * 269 * ONE),
            None,
            "above Nint 268"
        );
        assert!(PllDivider::new(4, 0, 0).is_err(), "SI2C is two bits");
        assert!(PllDivider::new(0, 64, 0).is_err(), "NI2C is six bits");
    }

    #[test]
    fn the_inputs_land_in_r20_to_r22() {
        // R820-DS-24/25 (Table 1-2): R20 = SI2C[7:6] NI2C[5:0], R21 = SDM_IN[8:1],
        // R22 = SDM_IN[16:9].
        let divider = PllDivider::new(0b10, 0b00_0101, 0x1234).unwrap();
        let mut registers = Registers::default();
        divider.write(&mut registers);
        assert_eq!(registers.byte(0x14), Some(0b1000_0101));
        assert_eq!(registers.byte(0x15), Some(0x34));
        assert_eq!(registers.byte(0x16), Some(0x12));
        assert_eq!(PllDivider::read(&registers), divider);
    }
}
