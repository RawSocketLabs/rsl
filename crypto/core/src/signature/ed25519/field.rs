//! Arithmetic in `GF(2^255 - 19)` for Edwards25519 points.
//!
//! RFC 8032 §5.1.1 defines the field and the inversion/square-root operations used by point
//! decoding. Five little-endian radix-`2^51` limbs make the reduction identity
//! `2^255 = 19 (mod p)` visible. This module is separate from X25519's field boundary because
//! Ed25519 requires canonical point encodings, square roots, signs, and conditional selection.

use zeroize::Zeroize;

const LIMB_BITS: u32 = 51;
const RADIX: u64 = 1_u64 << LIMB_BITS;
const LIMB_MASK: u64 = RADIX - 1;
const MODULUS: [u64; 5] = [LIMB_MASK - 18, LIMB_MASK, LIMB_MASK, LIMB_MASK, LIMB_MASK];

/// `(p - 5) / 8 = 2^252 - 3`, little-endian, for RFC 8032 §5.1.3 root recovery.
const ROOT_EXPONENT: [u8; 32] = [
    0xfd, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x0f,
];

/// RFC 8032 §5.1's `d = -121665 / 121666`, represented in radix `2^51`.
pub(super) const EDWARDS_D: [u64; 5] = [
    0x0003_4dca_1359_78a3,
    0x0001_a828_3b15_6ebd,
    0x0005_e7a2_6001_c029,
    0x0007_39c6_63a0_3cbb,
    0x0005_2036_cee2_b6ff,
];

/// Twice the Edwards curve constant, used by the complete addition formula in §5.1.4.
pub(super) const EDWARDS_2D: [u64; 5] = [
    0x0006_9b94_26b2_f159,
    0x0003_5050_762a_dd7a,
    0x0003_cf44_c003_8052,
    0x0006_738c_c740_7977,
    0x0002_406d_9dc5_6dff,
];

/// Square root of minus one selected by RFC 8032 §5.1.3 when the first root candidate fails.
const SQRT_MINUS_ONE: [u64; 5] = [
    0x0006_1b27_4a0e_a0b0,
    0x0000_d5a5_fc8f_189d,
    0x0007_ef5e_9cbd_0c60,
    0x0007_8595_a680_4c9e,
    0x0002_b832_4804_fc1d,
];

/// A field element capable of containing secret-derived Edwards coordinates.
///
/// It is intentionally non-`Copy`, non-`Clone`, non-formattable, and zeroizing.
pub(super) struct FieldElement {
    limbs: [u64; 5],
}

impl FieldElement {
    pub(super) const ZERO: Self = Self { limbs: [0; 5] };
    pub(super) const ONE: Self = Self {
        limbs: [1, 0, 0, 0, 0],
    };

    /// Construct a standard-derived constant already reduced into radix limbs.
    pub(super) const fn from_limbs(limbs: [u64; 5]) -> Self {
        Self { limbs }
    }

    /// Decode a canonical 255-bit little-endian field encoding.
    ///
    /// Ed25519 point decoding differs from X25519: RFC 8032 §5.1.3 requires failure when the
    /// encoded `y` is not in `0..p`, so this function never silently reduces a wire value.
    pub(super) fn from_canonical_bytes(bytes: &[u8; 32]) -> Option<Self> {
        if bytes[31] & 0x80 != 0 || !less_than_modulus(bytes) {
            return None;
        }
        Some(Self::from_reduced_bytes(bytes))
    }

    /// Decode bytes known to be below `p` into radix limbs.
    fn from_reduced_bytes(bytes: &[u8; 32]) -> Self {
        let w0 = load(bytes, 0);
        let w1 = load(bytes, 8);
        let w2 = load(bytes, 16);
        let w3 = load(bytes, 24);
        Self {
            limbs: [
                w0 & LIMB_MASK,
                ((w0 >> 51) | (w1 << 13)) & LIMB_MASK,
                ((w1 >> 38) | (w2 << 26)) & LIMB_MASK,
                ((w2 >> 25) | (w3 << 39)) & LIMB_MASK,
                (w3 >> 12) & LIMB_MASK,
            ],
        }
    }

    pub(super) fn add(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|i| {
            u128::from(self.limbs[i]) + u128::from(right.limbs[i])
        }))
    }

    pub(super) fn subtract(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|i| {
            u128::from(self.limbs[i]) + 2 * u128::from(MODULUS[i]) - u128::from(right.limbs[i])
        }))
    }

    pub(super) fn negate(&self) -> Self {
        Self::ZERO.subtract(self)
    }

    pub(super) fn multiply(&self, right: &Self) -> Self {
        let mut left = self.limbs.map(u128::from);
        let mut right = right.limbs.map(u128::from);
        let coefficients = [
            left[0] * right[0]
                + 19 * (left[1] * right[4]
                    + left[2] * right[3]
                    + left[3] * right[2]
                    + left[4] * right[1]),
            left[0] * right[1]
                + left[1] * right[0]
                + 19 * (left[2] * right[4] + left[3] * right[3] + left[4] * right[2]),
            left[0] * right[2]
                + left[1] * right[1]
                + left[2] * right[0]
                + 19 * (left[3] * right[4] + left[4] * right[3]),
            left[0] * right[3]
                + left[1] * right[2]
                + left[2] * right[1]
                + left[3] * right[0]
                + 19 * left[4] * right[4],
            left[0] * right[4]
                + left[1] * right[3]
                + left[2] * right[2]
                + left[3] * right[1]
                + left[4] * right[0],
        ];
        left.zeroize();
        right.zeroize();
        Self::from_coefficients(coefficients)
    }

    pub(super) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// Raise to the fixed public exponent `(p-5)/8` used in root recovery.
    fn pow_p58(&self) -> Self {
        let mut result = Self::ONE;
        for bit in (0..252).rev() {
            result = result.square();
            if (ROOT_EXPONENT[bit / 8] >> (bit % 8)) & 1 == 1 {
                result = result.multiply(self);
            }
        }
        result
    }

    /// Recover `sqrt(u/v)` with RFC 8032 §5.1.3's two-candidate procedure.
    pub(super) fn square_root_ratio(u: &Self, v: &Self) -> Option<Self> {
        let v2 = v.square();
        let v3 = v2.multiply(v);
        let v4 = v2.square();
        let v7 = v3.multiply(&v4);
        let uv7 = u.multiply(&v7);
        let mut x = u.multiply(&v3).multiply(&uv7.pow_p58());
        let vx2 = v.multiply(&x.square());

        if !vx2.equals(u) {
            if !vx2.equals(&u.negate()) {
                return None;
            }
            x = x.multiply(&Self::from_limbs(SQRT_MINUS_ONE));
        }
        Some(x)
    }

    /// Select `right` when `choice` is one and `left` when it is zero without branching.
    pub(super) fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        let mask = 0_u64.wrapping_sub(choice);
        Self {
            limbs: core::array::from_fn(|i| (left.limbs[i] & !mask) | (right.limbs[i] & mask)),
        }
    }

    pub(super) fn equals(&self, right: &Self) -> bool {
        self.to_bytes() == right.to_bytes()
    }
    pub(super) fn is_zero(&self) -> bool {
        self.to_bytes() == [0_u8; 32]
    }
    pub(super) fn is_negative(&self) -> bool {
        self.to_bytes()[0] & 1 == 1
    }

    /// Encode the unique representative below `p` in little-endian order.
    pub(super) fn to_bytes(&self) -> [u8; 32] {
        let mut limbs = canonical_limbs(self.limbs);
        let mut words = [
            limbs[0] | (limbs[1] << 51),
            (limbs[1] >> 13) | (limbs[2] << 38),
            (limbs[2] >> 26) | (limbs[3] << 25),
            (limbs[3] >> 39) | (limbs[4] << 12),
        ];
        let mut bytes = [0_u8; 32];
        for (i, word) in words.iter().enumerate() {
            bytes[i * 8..i * 8 + 8].copy_from_slice(&word.to_le_bytes());
        }
        limbs.zeroize();
        words.zeroize();
        bytes
    }

    fn from_coefficients(coefficients: [u128; 5]) -> Self {
        Self {
            limbs: reduce(coefficients),
        }
    }
}

impl Drop for FieldElement {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

fn load(bytes: &[u8; 32], start: usize) -> u64 {
    u64::from_le_bytes(
        bytes[start..start + 8]
            .try_into()
            .expect("field word is eight bytes"),
    )
}

fn less_than_modulus(bytes: &[u8; 32]) -> bool {
    const P_BYTES: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    for i in (0..32).rev() {
        if bytes[i] != P_BYTES[i] {
            return bytes[i] < P_BYTES[i];
        }
    }
    false
}

fn reduce(mut coefficients: [u128; 5]) -> [u64; 5] {
    let mask = u128::from(LIMB_MASK);
    for _ in 0..3 {
        for i in 0..4 {
            let carry = coefficients[i] >> LIMB_BITS;
            coefficients[i] &= mask;
            coefficients[i + 1] += carry;
        }
        let carry = coefficients[4] >> LIMB_BITS;
        coefficients[4] &= mask;
        coefficients[0] += carry * 19;
    }
    let output = coefficients.map(|value| {
        u64::try_from(value).expect("three carry passes leave every coefficient below 2^51")
    });
    coefficients.zeroize();
    output
}

fn canonical_limbs(limbs: [u64; 5]) -> [u64; 5] {
    let mut limbs = reduce(limbs.map(u128::from));
    let mut difference = [0_u64; 5];
    let mut borrow = 0_u64;
    for i in 0..5 {
        let tentative = limbs[i] + RADIX - MODULUS[i] - borrow;
        difference[i] = tentative & LIMB_MASK;
        borrow = 1 - (tentative >> LIMB_BITS);
    }
    let mask = 0_u64.wrapping_sub(1 - borrow);
    let output = core::array::from_fn(|i| (limbs[i] & !mask) | (difference[i] & mask));
    limbs.zeroize();
    difference.zeroize();
    output
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn canonical_boundary_rejects_p_and_accepts_p_minus_one() {
        let mut p = [0xff; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        let mut pm1 = p;
        pm1[0] -= 1;
        assert!(FieldElement::from_canonical_bytes(&p).is_none());
        assert_eq!(
            FieldElement::from_canonical_bytes(&pm1).unwrap().to_bytes(),
            pm1
        );
    }

    #[test]
    fn square_root_ratio_recovers_base_x_squared() {
        let y = FieldElement::from_limbs(super::super::point::BASE_Y);
        let y2 = y.square();
        let u = y2.subtract(&FieldElement::ONE);
        let v = FieldElement::from_limbs(EDWARDS_D)
            .multiply(&y2)
            .add(&FieldElement::ONE);
        let x = FieldElement::square_root_ratio(&u, &v).unwrap();
        assert!(v.multiply(&x.square()).equals(&u));
    }
}
