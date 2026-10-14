//! Arithmetic in `GF(2^448 - 2^224 - 1)` for edwards448 points.
//!
//! RFC 8032 §5.2.1 defines the field, the inversion `x^(p-2)`, and the square-root candidate
//! `(u/v)^((p+1)/4)` used by point decoding, computed with the §5.2.3 single-exponentiation
//! trick `u^3 v (u^5 v^3)^((p-3)/4)`. Eight little-endian radix-`2^56` limbs make the reduction
//! identity `2^448 = 2^224 + 1 (mod p)` visible. This module is separate from X448's field
//! boundary because Ed448 requires canonical encodings, square roots, signs, and conditional
//! selection.

use zeroize::Zeroize;

const LIMB_COUNT: usize = 8;
const LIMB_BITS: u32 = 56;
const RADIX: u64 = 1_u64 << LIMB_BITS;
const LIMB_MASK: u64 = RADIX - 1;

/// Bytes in a canonical field encoding (the 57th byte of a point encoding carries only the sign).
pub(super) const ELEMENT_BYTES: usize = 56;

const MODULUS_LIMBS: [u64; LIMB_COUNT] = [
    LIMB_MASK,
    LIMB_MASK,
    LIMB_MASK,
    LIMB_MASK,
    LIMB_MASK - 1,
    LIMB_MASK,
    LIMB_MASK,
    LIMB_MASK,
];

/// `p - 2`, little-endian, for inversion.
const INVERSE_EXPONENT: [u8; ELEMENT_BYTES] = {
    let mut bytes = [0xff_u8; ELEMENT_BYTES];
    bytes[0] = 0xfd;
    bytes[28] = 0xfe;
    bytes
};

/// `(p - 3) / 4`, little-endian, for the §5.2.3 root candidate.
const ROOT_EXPONENT: [u8; ELEMENT_BYTES] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xbf, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x3f,
];

/// RFC 8032 §5.2's `d = -39081`, in radix `2^56`.
pub(super) const EDWARDS_D: [u64; LIMB_COUNT] = [
    0xff_ffff_ffff_6756,
    0xff_ffff_ffff_ffff,
    0xff_ffff_ffff_ffff,
    0xff_ffff_ffff_ffff,
    0xff_ffff_ffff_fffe,
    0xff_ffff_ffff_ffff,
    0xff_ffff_ffff_ffff,
    0xff_ffff_ffff_ffff,
];

/// A field element capable of containing secret-derived Edwards coordinates.
pub(super) struct FieldElement {
    limbs: [u64; LIMB_COUNT],
}

impl FieldElement {
    pub(super) const ZERO: Self = Self { limbs: [0; 8] };
    pub(super) const ONE: Self = Self {
        limbs: [1, 0, 0, 0, 0, 0, 0, 0],
    };

    pub(super) const fn from_limbs(limbs: [u64; LIMB_COUNT]) -> Self {
        Self { limbs }
    }

    /// Decode a canonical 56-byte little-endian field encoding, rejecting values `>= p`.
    pub(super) fn from_canonical_bytes(bytes: &[u8; ELEMENT_BYTES]) -> Option<Self> {
        if !less_than_modulus(bytes) {
            return None;
        }
        Some(Self::from_reduced_bytes(bytes))
    }

    fn from_reduced_bytes(bytes: &[u8; ELEMENT_BYTES]) -> Self {
        let limbs = core::array::from_fn(|limb_index| {
            let mut limb = [0_u8; 8];
            limb[..7].copy_from_slice(&bytes[limb_index * 7..limb_index * 7 + 7]);
            u64::from_le_bytes(limb)
        });
        Self { limbs }
    }

    pub(super) fn add(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|i| {
            u128::from(self.limbs[i]) + u128::from(right.limbs[i])
        }))
    }

    pub(super) fn subtract(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|i| {
            u128::from(self.limbs[i]) + 2 * u128::from(MODULUS_LIMBS[i])
                - u128::from(right.limbs[i])
        }))
    }

    pub(super) fn negate(&self) -> Self {
        Self::ZERO.subtract(self)
    }

    /// Schoolbook multiplication folding `2^448 = 2^224 + 1`; see the X448 field for the
    /// weight analysis (weights 12–14 fold twice).
    pub(super) fn multiply(&self, right: &Self) -> Self {
        let mut left = self.limbs.map(u128::from);
        let mut right = right.limbs.map(u128::from);
        let mut coefficients = [0_u128; LIMB_COUNT];
        for (i, left_limb) in left.iter().enumerate() {
            for (j, right_limb) in right.iter().enumerate() {
                let product = left_limb * right_limb;
                let weight = i + j;
                if weight < LIMB_COUNT {
                    coefficients[weight] += product;
                } else if weight < LIMB_COUNT + 4 {
                    coefficients[weight - LIMB_COUNT] += product;
                    coefficients[weight - 4] += product;
                } else {
                    coefficients[weight - LIMB_COUNT] += 2 * product;
                    coefficients[weight - LIMB_COUNT - 4] += product;
                }
            }
        }
        left.zeroize();
        right.zeroize();
        Self::from_coefficients(coefficients)
    }

    pub(super) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// Fixed-schedule power by a public little-endian exponent.
    fn power(&self, exponent: &[u8; ELEMENT_BYTES]) -> Self {
        let mut result = Self::ONE;
        for bit in (0..448).rev() {
            result = result.square();
            if (exponent[bit / 8] >> (bit % 8)) & 1 == 1 {
                result = result.multiply(self);
            }
        }
        result
    }

    /// `z^(p-2)`.
    pub(super) fn invert(&self) -> Self {
        self.power(&INVERSE_EXPONENT)
    }

    /// RFC 8032 §5.2.3 steps 2–3: `x = u^3 v (u^5 v^3)^((p-3)/4)`, valid iff `v x^2 = u`.
    pub(super) fn square_root_ratio(u: &Self, v: &Self) -> Option<Self> {
        let u2 = u.square();
        let u3 = u2.multiply(u);
        let u5 = u3.multiply(&u2);
        let v3 = v.square().multiply(v);
        let x = u3
            .multiply(v)
            .multiply(&u5.multiply(&v3).power(&ROOT_EXPONENT));
        if v.multiply(&x.square()).equals(u) {
            Some(x)
        } else {
            None
        }
    }

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
        self.to_bytes() == [0_u8; ELEMENT_BYTES]
    }
    pub(super) fn is_negative(&self) -> bool {
        self.to_bytes()[0] & 1 == 1
    }

    /// Encode the unique representative below `p` in little-endian order.
    pub(super) fn to_bytes(&self) -> [u8; ELEMENT_BYTES] {
        let mut limbs = canonical_limbs(self.limbs);
        let mut out = [0_u8; ELEMENT_BYTES];
        for (i, limb) in limbs.iter().enumerate() {
            out[i * 7..i * 7 + 7].copy_from_slice(&limb.to_le_bytes()[..7]);
        }
        limbs.zeroize();
        out
    }

    fn from_coefficients(coefficients: [u128; LIMB_COUNT]) -> Self {
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

fn less_than_modulus(bytes: &[u8; ELEMENT_BYTES]) -> bool {
    let mut p = [0xff_u8; ELEMENT_BYTES];
    p[28] = 0xfe;
    for i in (0..ELEMENT_BYTES).rev() {
        if bytes[i] != p[i] {
            return bytes[i] < p[i];
        }
    }
    false
}

fn reduce(mut coefficients: [u128; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mask = u128::from(LIMB_MASK);
    for _ in 0..3 {
        for i in 0..LIMB_COUNT - 1 {
            let carry = coefficients[i] >> LIMB_BITS;
            coefficients[i] &= mask;
            coefficients[i + 1] += carry;
        }
        let carry = coefficients[LIMB_COUNT - 1] >> LIMB_BITS;
        coefficients[LIMB_COUNT - 1] &= mask;
        coefficients[0] += carry;
        coefficients[4] += carry;
    }
    let output = coefficients.map(|value| {
        u64::try_from(value).expect("three carry passes leave every coefficient below 2^56")
    });
    coefficients.zeroize();
    output
}

fn canonical_limbs(limbs: [u64; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mut limbs = reduce(limbs.map(u128::from));
    let mut difference = [0_u64; LIMB_COUNT];
    let mut borrow = 0_u64;
    for i in 0..LIMB_COUNT {
        let tentative = limbs[i] + RADIX - MODULUS_LIMBS[i] - borrow;
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
        let mut p = [0xff_u8; 56];
        p[28] = 0xfe;
        let mut pm1 = p;
        pm1[0] = 0xfe;
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
            .subtract(&FieldElement::ONE);
        let x = FieldElement::square_root_ratio(&u, &v).unwrap();
        assert!(v.multiply(&x.square()).equals(&u));
    }
}
