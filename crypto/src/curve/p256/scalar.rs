//! Residues modulo the P-256 group order `n`.
//!
//! ## Standards ownership
//!
//! SP 800-186 §3.2.1.3 publishes the prime order `n`. FIPS 186-5 §6.4.2 requires ECDSA
//! verification to reject `r` or `s` outside `[1, n-1]`, to reduce the hash integer `e` modulo
//! `n` through the products it enters, and to compute `s^-1 mod n`. SP 800-56A Rev. 3 §5.6.1.2
//! requires an ECC private key `d` to lie in `[1, n-1]`. This module owns those range rules and
//! the scalar arithmetic; it does not interpret what a scalar means.

use zeroize::Zeroize;

use super::arithmetic::{self, Limbs, Modulus};

/// SP 800-186 §3.2.1.3 group order `n`, little-endian limbs.
pub(crate) const ORDER: Modulus = Modulus::new([
    0xf3b9_cac2_fc63_2551,
    0xbce6_faad_a717_9e84,
    0xffff_ffff_ffff_ffff,
    0xffff_ffff_0000_0000,
]);

/// `n - 2`, the public inversion exponent.
const INVERSION_EXPONENT: Limbs = [
    0xf3b9_cac2_fc63_254f,
    0xbce6_faad_a717_9e84,
    0xffff_ffff_ffff_ffff,
    0xffff_ffff_0000_0000,
];

/// A residue modulo `n`, capable of holding a private key or per-signature value.
///
/// It is non-`Clone`, non-formattable, and zeroized on drop.
pub(crate) struct Scalar {
    limbs: Limbs,
}

impl Scalar {
    /// Decode a 32-byte big-endian integer in `[0, n-1]`, rejecting `n` and above.
    pub(crate) fn from_canonical_bytes(bytes: &[u8; 32]) -> Option<Self> {
        let limbs = arithmetic::from_be_bytes(bytes);
        if arithmetic::is_less_than(&limbs, &ORDER.value) {
            Some(Self { limbs })
        } else {
            None
        }
    }

    /// Decode a 32-byte big-endian integer in `[1, n-1]`, the private-key and `r`/`s` range.
    pub(crate) fn from_nonzero_canonical_bytes(bytes: &[u8; 32]) -> Option<Self> {
        let scalar = Self::from_canonical_bytes(bytes)?;
        if scalar.is_zero() { None } else { Some(scalar) }
    }

    /// Reduce any 256-bit big-endian integer modulo `n`.
    ///
    /// A 256-bit value is below `2n`, so one masked subtraction is a complete reduction. This
    /// is how a SHA-256 digest becomes ECDSA's `e` and how an `x`-coordinate becomes `v`.
    pub(crate) fn reduce_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            limbs: ORDER.reduce_once(&arithmetic::from_be_bytes(bytes)),
        }
    }

    /// Reduce a field-domain value (below `p`, hence below `2n`) modulo `n`.
    pub(crate) fn reduce_limbs(limbs: &Limbs) -> Self {
        Self {
            limbs: ORDER.reduce_once(limbs),
        }
    }

    /// Encode the canonical residue as a 32-byte big-endian integer.
    pub(crate) fn to_bytes(&self) -> [u8; 32] {
        arithmetic::to_be_bytes(&self.limbs)
    }

    pub(crate) fn multiply(&self, right: &Self) -> Self {
        Self {
            limbs: ORDER.multiply(&self.limbs, &right.limbs),
        }
    }

    /// `s^(n-2) = s^-1` for nonzero `s`.
    pub(crate) fn invert(&self) -> Self {
        Self {
            limbs: ORDER.power(&self.limbs, &INVERSION_EXPONENT),
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        arithmetic::is_zero(&self.limbs)
    }

    pub(crate) fn equals(&self, right: &Self) -> bool {
        self.limbs == right.limbs
    }
}

impl Drop for Scalar {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn n_bytes() -> [u8; 32] {
        arithmetic::to_be_bytes(&ORDER.value)
    }

    #[test]
    fn order_matches_the_published_hexadecimal_form() {
        let expected = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2,
            0xfc, 0x63, 0x25, 0x51,
        ];
        assert_eq!(n_bytes(), expected);
    }

    #[test]
    fn canonical_range_accepts_one_through_n_minus_one_only() {
        let n = n_bytes();
        let mut n_minus_one = n;
        n_minus_one[31] -= 1;
        let mut one = [0_u8; 32];
        one[31] = 1;
        assert!(Scalar::from_nonzero_canonical_bytes(&[0; 32]).is_none());
        assert!(Scalar::from_nonzero_canonical_bytes(&one).is_some());
        assert!(Scalar::from_nonzero_canonical_bytes(&n_minus_one).is_some());
        assert!(Scalar::from_nonzero_canonical_bytes(&n).is_none());
        assert!(Scalar::from_canonical_bytes(&[0; 32]).is_some());
    }

    #[test]
    fn reduction_maps_n_to_zero_and_all_ones_to_the_expected_remainder() {
        assert!(Scalar::reduce_bytes(&n_bytes()).is_zero());
        // 2^256 - 1 - n = (2^256 - n) - 1.
        let expected = {
            let (value, _) = arithmetic::subtract_limbs(&ORDER.complement, &[1, 0, 0, 0]);
            arithmetic::to_be_bytes(&value)
        };
        assert_eq!(Scalar::reduce_bytes(&[0xff; 32]).to_bytes(), expected);
    }

    #[test]
    fn inversion_multiplies_back_to_one() {
        let mut bytes = [0_u8; 32];
        bytes[31] = 7;
        let seven = Scalar::from_nonzero_canonical_bytes(&bytes).unwrap();
        let mut one = [0_u8; 32];
        one[31] = 1;
        assert_eq!(seven.multiply(&seven.invert()).to_bytes(), one);
    }
}
