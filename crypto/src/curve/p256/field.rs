//! Residues modulo the P-256 field prime `p`.
//!
//! ## Standards ownership
//!
//! SP 800-186 §3.2.1.3 publishes `p = 2^256 - 2^224 + 2^192 + 2^96 - 1` and the coefficient
//! `b`; SEC 1 §2.3.5 fixes the field-element-to-octet-string conversion as a 32-byte big-endian
//! integer, and §2.3.6 rejects octet strings whose integer is not below `p`. This module owns
//! those rules and the inversion `z^(p-2)` used to recover affine coordinates.

use zeroize::Zeroize;

use super::arithmetic::{self, Limbs, Modulus};

/// SP 800-186 §3.2.1.3 field prime `p`, little-endian limbs.
pub(crate) const MODULUS: Modulus = Modulus::new([
    0xffff_ffff_ffff_ffff,
    0x0000_0000_ffff_ffff,
    0x0000_0000_0000_0000,
    0xffff_ffff_0000_0001,
]);

/// SP 800-186 §3.2.1.3 coefficient `b`, little-endian limbs.
pub(crate) const CURVE_B: Limbs = [
    0x3bce_3c3e_27d2_604b,
    0x651d_06b0_cc53_b0f6,
    0xb3eb_bd55_7698_86bc,
    0x5ac6_35d8_aa3a_93e7,
];

/// `p - 2`, the public inversion exponent.
const INVERSION_EXPONENT: Limbs = [
    0xffff_ffff_ffff_fffd,
    0x0000_0000_ffff_ffff,
    0x0000_0000_0000_0000,
    0xffff_ffff_0000_0001,
];

/// A residue modulo `p`, capable of holding a secret-derived coordinate.
///
/// It is non-`Clone`, non-formattable, and zeroized on drop.
pub(crate) struct FieldElement {
    limbs: Limbs,
}

impl FieldElement {
    pub(crate) const ZERO: Self = Self { limbs: [0; 4] };
    pub(crate) const ONE: Self = Self {
        limbs: [1, 0, 0, 0],
    };

    /// Wrap limbs already known to be below `p`, such as published constants.
    pub(crate) const fn from_limbs(limbs: Limbs) -> Self {
        Self { limbs }
    }

    /// SEC 1 §2.3.6: decode a 32-byte big-endian integer, rejecting values `>= p`.
    pub(crate) fn from_canonical_bytes(bytes: &[u8; 32]) -> Option<Self> {
        let limbs = arithmetic::from_be_bytes(bytes);
        if arithmetic::is_less_than(&limbs, &MODULUS.value) {
            Some(Self { limbs })
        } else {
            None
        }
    }

    /// SEC 1 §2.3.5: encode the residue as a 32-byte big-endian integer.
    pub(crate) fn to_bytes(&self) -> [u8; 32] {
        arithmetic::to_be_bytes(&self.limbs)
    }

    pub(crate) fn add(&self, right: &Self) -> Self {
        Self {
            limbs: MODULUS.add(&self.limbs, &right.limbs),
        }
    }

    pub(crate) fn subtract(&self, right: &Self) -> Self {
        Self {
            limbs: MODULUS.subtract(&self.limbs, &right.limbs),
        }
    }

    pub(crate) fn multiply(&self, right: &Self) -> Self {
        Self {
            limbs: MODULUS.multiply(&self.limbs, &right.limbs),
        }
    }

    pub(crate) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// `z^(p-2) = z^-1` for nonzero `z`; zero maps to zero.
    pub(crate) fn invert(&self) -> Self {
        Self {
            limbs: MODULUS.power(&self.limbs, &INVERSION_EXPONENT),
        }
    }

    /// Select `right` when `choice` is one and `left` when it is zero without branching.
    pub(crate) fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        Self {
            limbs: arithmetic::select(&left.limbs, &right.limbs, choice),
        }
    }

    pub(crate) fn equals(&self, right: &Self) -> bool {
        self.limbs == right.limbs
    }

    pub(crate) fn is_zero(&self) -> bool {
        arithmetic::is_zero(&self.limbs)
    }

    /// Borrow the canonical limbs for a scalar-domain reinterpretation of `x`.
    pub(crate) const fn limbs(&self) -> &Limbs {
        &self.limbs
    }
}

impl Drop for FieldElement {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn p_bytes() -> [u8; 32] {
        arithmetic::to_be_bytes(&MODULUS.value)
    }

    #[test]
    fn modulus_matches_the_published_hexadecimal_form() {
        let expected = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
        ];
        assert_eq!(p_bytes(), expected);
    }

    #[test]
    fn canonical_decoding_rejects_p_and_accepts_p_minus_one() {
        let p = p_bytes();
        let mut p_minus_one = p;
        p_minus_one[31] -= 1;
        assert!(FieldElement::from_canonical_bytes(&p).is_none());
        assert!(FieldElement::from_canonical_bytes(&[0xff; 32]).is_none());
        assert_eq!(
            FieldElement::from_canonical_bytes(&p_minus_one)
                .unwrap()
                .to_bytes(),
            p_minus_one
        );
    }

    #[test]
    fn inversion_of_b_multiplies_back_to_one_and_zero_stays_zero() {
        let b = FieldElement::from_limbs(CURVE_B);
        assert!(b.multiply(&b.invert()).equals(&FieldElement::ONE));
        assert!(FieldElement::ZERO.invert().is_zero());
    }

    #[test]
    fn conditional_select_picks_exactly_one_operand() {
        let b = FieldElement::from_limbs(CURVE_B);
        assert!(
            FieldElement::conditional_select(&FieldElement::ONE, &b, 0).equals(&FieldElement::ONE)
        );
        assert!(FieldElement::conditional_select(&FieldElement::ONE, &b, 1).equals(&b));
    }
}
