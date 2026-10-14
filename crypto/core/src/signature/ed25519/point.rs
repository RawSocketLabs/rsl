//! Edwards25519 point encoding, decoding, addition, and scalar multiplication.
//!
//! RFC 8032 §§5.1.2–5.1.4 encode a point as canonical little-endian `y` plus the low bit of `x`,
//! recover `x` by a checked square root, and give complete extended-coordinate addition formulas.
//! Scalar multiplication here always executes 256 additions, 256 doublings, and masked selects;
//! this makes the source structure independent of secret scalar bits without claiming the final
//! machine code has received a side-channel audit.

use super::field::{EDWARDS_2D, EDWARDS_D, FieldElement};

/// Standard Ed25519 basepoint x-coordinate in radix `2^51`.
const BASE_X: [u64; 5] = [
    0x0006_2d60_8f25_d51a,
    0x0004_12a4_b4f6_592a,
    0x0007_5b71_71a4_b31d,
    0x0001_ff60_5271_18fe,
    0x0002_1693_6d3c_d6e5,
];

/// Standard Ed25519 basepoint y-coordinate `4/5` in radix `2^51`.
pub(super) const BASE_Y: [u64; 5] = [
    0x0006_6666_6666_6658,
    0x0004_cccc_cccc_cccc,
    0x0001_9999_9999_9999,
    0x0003_3333_3333_3333,
    0x0006_6666_6666_6666,
];

/// A point represented as RFC 8032 §5.1.4 extended coordinates `(X,Y,Z,T)`.
pub(super) struct EdwardsPoint {
    x: FieldElement,
    y: FieldElement,
    z: FieldElement,
    t: FieldElement,
}

impl EdwardsPoint {
    /// Neutral group element `(0,1,1,0)`.
    pub(super) const fn identity() -> Self {
        Self {
            x: FieldElement::ZERO,
            y: FieldElement::ONE,
            z: FieldElement::ONE,
            t: FieldElement::ZERO,
        }
    }

    /// RFC 8032 §5.1's fixed Ed25519 basepoint.
    pub(super) fn basepoint() -> Self {
        let x = FieldElement::from_limbs(BASE_X);
        let y = FieldElement::from_limbs(BASE_Y);
        let t = x.multiply(&y);
        Self {
            x,
            y,
            z: FieldElement::ONE,
            t,
        }
    }

    /// Decode canonical `y`, recover `x`, and enforce the encoded sign bit.
    #[allow(clippy::many_single_char_names)] // `x`, `y`, `u`, `v`, and `T` preserve RFC 8032.
    pub(super) fn decompress(bytes: &[u8; 32]) -> Option<Self> {
        let sign = u64::from(bytes[31] >> 7);
        let mut y_bytes = *bytes;
        y_bytes[31] &= 0x7f;
        let y = FieldElement::from_canonical_bytes(&y_bytes)?;
        let y_squared = y.square();
        let u = y_squared.subtract(&FieldElement::ONE);
        let v = FieldElement::from_limbs(EDWARDS_D)
            .multiply(&y_squared)
            .add(&FieldElement::ONE);
        let mut x = FieldElement::square_root_ratio(&u, &v)?;

        // RFC 8032 §5.1.3 rejects the otherwise ambiguous negative encoding of x=0.
        if x.is_zero() && sign == 1 {
            return None;
        }
        if u64::from(x.is_negative()) != sign {
            x = x.negate();
        }
        let t = x.multiply(&y);
        Some(Self {
            x,
            y,
            z: FieldElement::ONE,
            t,
        })
    }

    /// Encode affine `y` and the low bit of affine `x` as RFC 8032 §5.1.2 specifies.
    pub(super) fn compress(&self) -> [u8; 32] {
        let inverse_z = self.z.invert();
        let affine_x = self.x.multiply(&inverse_z);
        let affine_y = self.y.multiply(&inverse_z);
        let mut bytes = affine_y.to_bytes();
        bytes[31] |= u8::from(affine_x.is_negative()) << 7;
        bytes
    }

    /// Complete extended-coordinate addition in the exact `A..H` order of RFC 8032 §5.1.4.
    #[allow(clippy::many_single_char_names)] // `A` through `H` are the standard's intermediates.
    pub(super) fn add(&self, right: &Self) -> Self {
        let a = self
            .y
            .subtract(&self.x)
            .multiply(&right.y.subtract(&right.x));
        let b = self.y.add(&self.x).multiply(&right.y.add(&right.x));
        let c = self
            .t
            .multiply(&FieldElement::from_limbs(EDWARDS_2D))
            .multiply(&right.t);
        let d = self.z.add(&self.z).multiply(&right.z);
        let e = b.subtract(&a);
        let f = d.subtract(&c);
        let g = d.add(&c);
        let h = b.add(&a);
        Self {
            x: e.multiply(&f),
            y: g.multiply(&h),
            t: e.multiply(&h),
            z: f.multiply(&g),
        }
    }

    /// Doubling uses the complete addition path with equal operands.
    pub(super) fn double(&self) -> Self {
        self.add(self)
    }

    /// Fixed-structure double-and-add multiplication over all 256 encoded scalar bits.
    pub(super) fn multiply(&self, scalar: &[u8; 32]) -> Self {
        let mut result = Self::identity();
        // A masked select with identical operands makes the additional owner explicit without
        // giving secret-capable points a general `Clone` implementation.
        let mut addend = Self::conditional_select(self, self, 0);

        for bit_index in 0..256 {
            let sum = result.add(&addend);
            let bit = u64::from((scalar[bit_index / 8] >> (bit_index % 8)) & 1);
            result = Self::conditional_select(&result, &sum, bit);
            addend = addend.double();
        }
        result
    }

    /// Multiply by the public cofactor eight using three doublings.
    pub(super) fn multiply_by_cofactor(&self) -> Self {
        self.double().double().double()
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
            t: FieldElement::conditional_select(&left.t, &right.t, choice),
        }
    }
}

// Inversion belongs on the field type but is used first at the point-encoding boundary.
impl FieldElement {
    /// Calculate `z^(p-2)` by a fixed public square-and-multiply schedule.
    pub(super) fn invert(&self) -> Self {
        const EXPONENT: [u8; 32] = [
            0xeb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0x7f,
        ];
        let mut result = Self::ONE;
        for bit in (0..255).rev() {
            result = result.square();
            if (EXPONENT[bit / 8] >> (bit % 8)) & 1 == 1 {
                result = result.multiply(self);
            }
        }
        result
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn basepoint_has_the_published_compressed_encoding() {
        let mut expected = [0x66_u8; 32];
        expected[0] = 0x58;
        assert_eq!(EdwardsPoint::basepoint().compress(), expected);
    }

    #[test]
    fn compression_and_decompression_preserve_varied_multiples() {
        let base = EdwardsPoint::basepoint();
        for scalar_byte in 0_u8..32 {
            let mut scalar = [0_u8; 32];
            scalar[0] = scalar_byte;
            let encoded = base.multiply(&scalar).compress();
            assert_eq!(
                EdwardsPoint::decompress(&encoded).unwrap().compress(),
                encoded
            );
        }
    }

    #[test]
    fn identity_is_small_order_but_basepoint_is_not() {
        assert!(EdwardsPoint::identity().is_small_order());
        assert!(!EdwardsPoint::basepoint().is_small_order());
    }

    #[test]
    fn negative_zero_and_noncanonical_y_are_rejected() {
        let mut negative_zero = [0_u8; 32];
        negative_zero[0] = 1;
        negative_zero[31] = 0x80;
        assert!(EdwardsPoint::decompress(&negative_zero).is_none());
        let mut p = [0xff; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        assert!(EdwardsPoint::decompress(&p).is_none());
    }
}
