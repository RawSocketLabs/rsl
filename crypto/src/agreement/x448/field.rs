//! Arithmetic in the X448 field `GF(2^448 - 2^224 - 1)`.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §4.2][rfc-7748] fixes the prime `p = 2^448 - 2^224 - 1`. Section 5 requires
//! little-endian 56-byte coordinate decoding, accepting non-canonical values (`p` through
//! `2^448 - 1`) as their residue modulo `p`, field arithmetic modulo `p`, and canonical output.
//! Unlike X25519 there is no unused high bit to mask.
//!
//! This layer represents a field element as eight little-endian radix-`2^56` limbs. Eight limbs
//! cover 448 bits exactly, `u128` products hold every unreduced sum, and the relation
//! `2^448 = 2^224 + 1 (mod p)` folds a carry out of limb seven back into limb zero *and* limb
//! four (`2^224` is the boundary of limb four, since `4 · 56 = 224`).
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use zeroize::Zeroize;

/// Number of radix-`2^56` limbs in a field element.
const LIMB_COUNT: usize = 8;

/// One radix limb's bit width.
const LIMB_BITS: u32 = 56;

/// The radix value `2^56`.
const RADIX: u64 = 1_u64 << LIMB_BITS;

/// Mask retaining the low 56 bits of a limb.
const LIMB_MASK: u64 = RADIX - 1;

/// Bytes in one encoded coordinate.
pub(super) const ELEMENT_BYTES: usize = 56;

/// The prime `2^448 - 2^224 - 1` split into eight radix-`2^56` limbs: all ones except that limb
/// four (weight `2^224`) is one short.
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

/// The fixed exponent `p - 2`, encoded little-endian.
///
/// `p - 2 = 2^448 - 2^224 - 3`: bytes 0..28 are `ff` except byte 0 (`fd`), byte 28 is `fe`
/// (the `-2^224` borrow), and bytes 29..56 are `ff`.
const INVERSE_EXPONENT: [u8; ELEMENT_BYTES] = {
    let mut bytes = [0xff_u8; ELEMENT_BYTES];
    bytes[0] = 0xfd;
    bytes[28] = 0xfe;
    bytes
};

/// One secret-capable field element in little-endian radix-`2^56` form.
pub(super) struct FieldElement {
    limbs: [u64; LIMB_COUNT],
}

impl FieldElement {
    pub(super) const ZERO: Self = Self {
        limbs: [0; LIMB_COUNT],
    };

    pub(super) const ONE: Self = Self {
        limbs: [1, 0, 0, 0, 0, 0, 0, 0],
    };

    /// Decode one RFC 7748 X448 u-coordinate: 56 little-endian bytes, seven per limb.
    #[must_use]
    pub(super) fn from_bytes(bytes: &[u8; ELEMENT_BYTES]) -> Self {
        let limbs = core::array::from_fn(|limb_index| {
            let mut limb = [0_u8; 8];
            limb[..7].copy_from_slice(&bytes[limb_index * 7..limb_index * 7 + 7]);
            u64::from_le_bytes(limb)
        });
        Self { limbs }
    }

    #[must_use]
    pub(super) fn add(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|index| {
            u128::from(self.limbs[index]) + u128::from(right.limbs[index])
        }))
    }

    /// Subtract modulo `p`; adding `2p` first keeps every coefficient nonnegative.
    #[must_use]
    pub(super) fn subtract(&self, right: &Self) -> Self {
        Self::from_coefficients(core::array::from_fn(|index| {
            u128::from(self.limbs[index]) + 2 * u128::from(MODULUS_LIMBS[index])
                - u128::from(right.limbs[index])
        }))
    }

    /// Schoolbook multiplication with the `2^448 = 2^224 + 1` fold applied to high terms.
    ///
    /// The product of limbs `i` and `j` has weight `2^(56·w)`, `w = i + j`. For `8 <= w <= 11`
    /// the factor `2^448` folds to `2^224 + 1`, so the product lands in coefficients `w - 8`
    /// and `w - 4`. For `12 <= w <= 14` the `2^224` term itself carries weight `>= 2^448` and
    /// folds again, so the product lands twice in `w - 8` and once in `w - 12`. Each
    /// coefficient accumulates at most a few dozen 112-bit products, well inside `u128`.
    #[must_use]
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

    #[must_use]
    pub(super) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// Multiply by a small public constant such as `a24 = 39081`.
    #[must_use]
    pub(super) fn multiply_small(&self, value: u64) -> Self {
        Self::from_coefficients(self.limbs.map(|limb| u128::from(limb) * u128::from(value)))
    }

    /// `self^(p-2)` through a fixed public square-and-multiply schedule over 448 bits.
    #[must_use]
    pub(super) fn invert(&self) -> Self {
        let mut result = Self::ONE;
        for bit_index in (0..448).rev() {
            result = result.square();
            if (INVERSE_EXPONENT[bit_index / 8] >> (bit_index % 8)) & 1 == 1 {
                result = result.multiply(self);
            }
        }
        result
    }

    /// Encode the unique canonical little-endian representative in `[0, p)`.
    #[must_use]
    pub(super) fn to_bytes(&self) -> [u8; ELEMENT_BYTES] {
        let mut limbs = canonical_limbs(self.limbs);
        let mut output = [0_u8; ELEMENT_BYTES];
        for (limb_index, limb) in limbs.iter().enumerate() {
            output[limb_index * 7..limb_index * 7 + 7].copy_from_slice(&limb.to_le_bytes()[..7]);
        }
        limbs.zeroize();
        output
    }

    /// Swap two field elements using RFC 7748 §5's `mask(swap) = 0 - swap` construction.
    pub(super) fn conditional_swap(swap: u64, left: &mut Self, right: &mut Self) {
        let mask = 0_u64.wrapping_sub(swap);
        for limb_index in 0..LIMB_COUNT {
            let selected = mask & (left.limbs[limb_index] ^ right.limbs[limb_index]);
            left.limbs[limb_index] ^= selected;
            right.limbs[limb_index] ^= selected;
        }
    }

    #[must_use]
    fn from_coefficients(coefficients: [u128; LIMB_COUNT]) -> Self {
        Self {
            limbs: reduce_coefficients(coefficients),
        }
    }
}

impl Drop for FieldElement {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

/// Propagate radix carries and fold every `2^448` multiple back as `2^224 + 1`.
#[must_use]
fn reduce_coefficients(mut coefficients: [u128; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mask = u128::from(LIMB_MASK);
    // Three fixed passes: the first reduces raw product coefficients, the second propagates the
    // folded high carry (which lands in limbs zero and four), and the third resolves the ripple.
    for _ in 0..3 {
        for limb_index in 0..LIMB_COUNT - 1 {
            let carry = coefficients[limb_index] >> LIMB_BITS;
            coefficients[limb_index] &= mask;
            coefficients[limb_index + 1] += carry;
        }
        let high_carry = coefficients[LIMB_COUNT - 1] >> LIMB_BITS;
        coefficients[LIMB_COUNT - 1] &= mask;
        coefficients[0] += high_carry;
        coefficients[4] += high_carry;
    }
    let output = coefficients.map(|coefficient| {
        u64::try_from(coefficient).expect("three carry passes leave every coefficient below 2^56")
    });
    coefficients.zeroize();
    output
}

/// Conditionally subtract `p` so the returned limbs encode a value below the modulus.
#[must_use]
fn canonical_limbs(limbs: [u64; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mut limbs = reduce_coefficients(limbs.map(u128::from));
    let mut difference = [0_u64; LIMB_COUNT];
    let mut borrow = 0_u64;
    for limb_index in 0..LIMB_COUNT {
        let tentative = limbs[limb_index] + RADIX - MODULUS_LIMBS[limb_index] - borrow;
        difference[limb_index] = tentative & LIMB_MASK;
        borrow = 1 - (tentative >> LIMB_BITS);
    }
    let use_difference = 1_u64.wrapping_sub(borrow);
    let mask = 0_u64.wrapping_sub(use_difference);
    let output = core::array::from_fn(|index| (limbs[index] & !mask) | (difference[index] & mask));
    limbs.zeroize();
    difference.zeroize();
    output
}

#[cfg(test)]
mod unit {
    use super::*;

    fn p_bytes() -> [u8; 56] {
        let mut p = [0xff_u8; 56];
        p[28] = 0xfe;
        p
    }

    /// Standard-derived evidence: `p` encodes as zero, `p + 5` as five, and `2^448 - 1` as
    /// `2^224` (its residue), so RFC 7748 §5's non-canonical inputs are processed, not rejected.
    #[test]
    fn noncanonical_inputs_reduce_modulo_p() {
        assert_eq!(FieldElement::from_bytes(&p_bytes()).to_bytes(), [0; 56]);
        // p + 5 = 2^448 - 2^224 + 4: byte 0 is 0x04, bytes 1..28 are zero, bytes 28..56 are 0xff.
        let mut p_plus_five = [0_u8; 56];
        p_plus_five[0] = 0x04;
        p_plus_five[28..].fill(0xff);
        let mut five = [0_u8; 56];
        five[0] = 5;
        assert_eq!(FieldElement::from_bytes(&p_plus_five).to_bytes(), five);
        let reduced = FieldElement::from_bytes(&[0xff; 56]).to_bytes();
        let mut two_pow_224 = [0_u8; 56];
        two_pow_224[28] = 1;
        assert_eq!(reduced, two_pow_224);
    }

    #[test]
    fn multiplication_inversion_and_subtraction_agree_on_small_values() {
        let mut seven = [0_u8; 56];
        seven[0] = 7;
        let x = FieldElement::from_bytes(&seven);
        let mut one = [0_u8; 56];
        one[0] = 1;
        assert_eq!(x.multiply(&x.invert()).to_bytes(), one);
        assert_eq!(x.subtract(&x).to_bytes(), [0; 56]);
        let mut forty_nine = [0_u8; 56];
        forty_nine[0] = 49;
        assert_eq!(x.square().to_bytes(), forty_nine);
        assert_eq!(
            x.multiply_small(39_081).to_bytes()[..3],
            (7_u64 * 39_081).to_le_bytes()[..3]
        );
    }

    #[test]
    fn p_minus_one_squared_is_one() {
        let mut p_minus_one = p_bytes();
        p_minus_one[0] = 0xfe;
        let x = FieldElement::from_bytes(&p_minus_one);
        let mut one = [0_u8; 56];
        one[0] = 1;
        assert_eq!(x.square().to_bytes(), one);
    }
}
