//! Edwards448 point encoding, decoding, addition, doubling, and scalar multiplication.
//!
//! RFC 8032 §§5.2.2–5.2.4 encode a point as canonical little-endian `y` in 57 bytes plus the
//! low bit of `x` in the final byte's top bit, recover `x` by a checked square root, and give
//! projective `(X, Y, Z)` addition and doubling formulas for the untwisted curve
//! `x^2 + y^2 = 1 + d x^2 y^2`. Those formulas are complete for this non-square `d`. Scalar
//! multiplication always executes 456 additions, 456 doublings, and masked selects.

use super::field::{EDWARDS_D, ELEMENT_BYTES, FieldElement};

/// Bytes in an encoded point.
pub(super) const ENCODED_BYTES: usize = 57;

/// RFC 7748 §4.2 edwards448 base point `X(P)` in radix `2^56`.
const BASE_X: [u64; 8] = [
    0x26_a82b_c70c_c05e,
    0x80_e18b_0093_8e26,
    0xf7_2ab6_6511_433b,
    0xa3_d3a4_6412_ae1a,
    0x0f_1767_ea6d_e324,
    0x36_da9e_1465_7047,
    0xed_221d_15a6_22bf,
    0x4f_1970_c66b_ed0d,
];

/// RFC 7748 §4.2 edwards448 base point `Y(P)` in radix `2^56`.
pub(super) const BASE_Y: [u64; 8] = [
    0x08_795b_f230_fa14,
    0x13_2c4e_d7c8_ad98,
    0x1c_e67c_39c4_fdbd,
    0x05_a0c2_d73a_d3ff,
    0xa3_9840_8778_9c1e,
    0xc7_624b_ea73_736c,
    0x24_8876_2037_56c9,
    0x69_3f46_716e_b6bc,
];

/// A point in RFC 8032 §5.2.4 projective coordinates `(X, Y, Z)`.
pub(super) struct EdwardsPoint {
    x: FieldElement,
    y: FieldElement,
    z: FieldElement,
}

impl EdwardsPoint {
    /// Neutral element `(0, 1)` as `(0, 1, 1)`.
    pub(super) const fn identity() -> Self {
        Self {
            x: FieldElement::ZERO,
            y: FieldElement::ONE,
            z: FieldElement::ONE,
        }
    }

    /// The fixed base point `B`.
    pub(super) const fn basepoint() -> Self {
        Self {
            x: FieldElement::from_limbs(BASE_X),
            y: FieldElement::from_limbs(BASE_Y),
            z: FieldElement::ONE,
        }
    }

    /// §5.2.3: decode canonical `y`, recover `x`, and enforce the sign bit.
    #[allow(clippy::many_single_char_names)] // `x`, `y`, `u`, and `v` preserve RFC 8032.
    pub(super) fn decompress(bytes: &[u8; ENCODED_BYTES]) -> Option<Self> {
        let sign = u64::from(bytes[56] >> 7);
        // The final byte carries only the sign bit; any other set bit is not a canonical `y`.
        if bytes[56] & 0x7f != 0 {
            return None;
        }
        let y_bytes: [u8; ELEMENT_BYTES] = bytes[..56].try_into().expect("56 bytes of y");
        let y = FieldElement::from_canonical_bytes(&y_bytes)?;
        let y_squared = y.square();
        let u = y_squared.subtract(&FieldElement::ONE);
        let v = FieldElement::from_limbs(EDWARDS_D)
            .multiply(&y_squared)
            .subtract(&FieldElement::ONE);
        let mut x = FieldElement::square_root_ratio(&u, &v)?;
        if x.is_zero() && sign == 1 {
            return None;
        }
        if u64::from(x.is_negative()) != sign {
            x = x.negate();
        }
        Some(Self {
            x,
            y,
            z: FieldElement::ONE,
        })
    }

    /// §5.2.2: encode affine `y` and the low bit of affine `x`.
    pub(super) fn compress(&self) -> [u8; ENCODED_BYTES] {
        let inverse_z = self.z.invert();
        let affine_x = self.x.multiply(&inverse_z);
        let affine_y = self.y.multiply(&inverse_z);
        let mut bytes = [0_u8; ENCODED_BYTES];
        bytes[..56].copy_from_slice(&affine_y.to_bytes());
        bytes[56] = u8::from(affine_x.is_negative()) << 7;
        bytes
    }

    /// §5.2.4 addition in the printed `A..H` order.
    #[allow(clippy::many_single_char_names)] // `A` through `H` are the standard's intermediates.
    pub(super) fn add(&self, right: &Self) -> Self {
        let a = self.z.multiply(&right.z);
        let b = a.square();
        let c = self.x.multiply(&right.x);
        let d = self.y.multiply(&right.y);
        let e = FieldElement::from_limbs(EDWARDS_D)
            .multiply(&c)
            .multiply(&d);
        let f = b.subtract(&e);
        let g = b.add(&e);
        let h = self.x.add(&self.y).multiply(&right.x.add(&right.y));
        Self {
            x: a.multiply(&f).multiply(&h.subtract(&c).subtract(&d)),
            y: a.multiply(&g).multiply(&d.subtract(&c)),
            z: f.multiply(&g),
        }
    }

    /// §5.2.4 doubling in the printed `B..J` order.
    #[allow(clippy::many_single_char_names)] // `B` through `J` are the standard's intermediates.
    pub(super) fn double(&self) -> Self {
        let b = self.x.add(&self.y).square();
        let c = self.x.square();
        let d = self.y.square();
        let e = c.add(&d);
        let h = self.z.square();
        let j = e.subtract(&h.add(&h));
        Self {
            x: b.subtract(&e).multiply(&j),
            y: e.multiply(&c.subtract(&d)),
            z: e.multiply(&j),
        }
    }

    /// Fixed-structure double-and-add over all 456 bits of a 57-byte little-endian scalar.
    pub(super) fn multiply(&self, scalar: &[u8; 57]) -> Self {
        let mut result = Self::identity();
        let mut addend = Self::conditional_select(self, self, 0);
        for bit_index in 0..456 {
            let sum = result.add(&addend);
            let bit = u64::from((scalar[bit_index / 8] >> (bit_index % 8)) & 1);
            result = Self::conditional_select(&result, &sum, bit);
            addend = addend.double();
        }
        result
    }

    /// Multiply by the public cofactor four using two doublings.
    pub(super) fn multiply_by_cofactor(&self) -> Self {
        self.double().double()
    }

    pub(super) fn equals(&self, right: &Self) -> bool {
        self.x.multiply(&right.z).equals(&right.x.multiply(&self.z))
            && self.y.multiply(&right.z).equals(&right.y.multiply(&self.z))
    }

    pub(super) fn is_identity(&self) -> bool {
        self.equals(&Self::identity())
    }
    pub(super) fn is_small_order(&self) -> bool {
        self.multiply_by_cofactor().is_identity()
    }

    fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        Self {
            x: FieldElement::conditional_select(&left.x, &right.x, choice),
            y: FieldElement::conditional_select(&left.y, &right.y, choice),
            z: FieldElement::conditional_select(&left.z, &right.z, choice),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn basepoint_round_trips_through_compression_and_is_not_small_order() {
        let encoded = EdwardsPoint::basepoint().compress();
        assert_eq!(
            EdwardsPoint::decompress(&encoded).unwrap().compress(),
            encoded
        );
        assert!(!EdwardsPoint::basepoint().is_small_order());
        assert!(EdwardsPoint::identity().is_small_order());
    }

    #[test]
    fn doubling_matches_addition_and_multiples_round_trip() {
        let base = EdwardsPoint::basepoint();
        assert!(base.double().equals(&base.add(&base)));
        for scalar_byte in 0_u8..16 {
            let mut scalar = [0_u8; 57];
            scalar[0] = scalar_byte;
            let encoded = base.multiply(&scalar).compress();
            assert_eq!(
                EdwardsPoint::decompress(&encoded).unwrap().compress(),
                encoded
            );
        }
    }

    #[test]
    fn negative_zero_noncanonical_y_and_stray_bits_are_rejected() {
        let mut negative_zero = [0_u8; 57];
        negative_zero[0] = 1;
        negative_zero[56] = 0x80;
        assert!(EdwardsPoint::decompress(&negative_zero).is_none());
        let mut p = [0xff_u8; 57];
        p[28] = 0xfe;
        p[56] = 0;
        assert!(EdwardsPoint::decompress(&p).is_none());
        let mut stray = EdwardsPoint::basepoint().compress();
        stray[56] |= 0x01;
        assert!(EdwardsPoint::decompress(&stray).is_none());
    }
}
