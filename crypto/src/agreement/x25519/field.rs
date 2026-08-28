//! Arithmetic in the X25519 field `GF(2^255 - 19)`.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §4.1][rfc-7748] fixes the prime `p = 2^255 - 19`. Section 5 requires
//! little-endian 32-byte coordinate decoding, masking the top input bit, accepting non-canonical
//! values as their residue modulo `p`, field arithmetic modulo `p`, and canonical output.
//!
//! This layer represents a field element as five little-endian radix-`2^51` limbs. The choice is
//! an implementation mapping rather than RFC notation: five limbs cover 255 bits exactly, while
//! `u128` products hold every unreduced sum without overflow. The relation `2^255 = 19 (mod p)`
//! folds a carry from limb four back into limb zero multiplied by nineteen.
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use zeroize::Zeroize;

/// Number of radix-`2^51` limbs in a field element.
const LIMB_COUNT: usize = 5;

/// One radix limb's bit width.
const LIMB_BITS: u32 = 51;

/// The radix value `2^51`.
const RADIX: u64 = 1_u64 << LIMB_BITS;

/// Mask retaining the low 51 bits of a limb.
const LIMB_MASK: u64 = RADIX - 1;

/// The prime `2^255 - 19` split into five radix-`2^51` limbs.
const MODULUS_LIMBS: [u64; LIMB_COUNT] =
    [LIMB_MASK - 18, LIMB_MASK, LIMB_MASK, LIMB_MASK, LIMB_MASK];

/// The fixed exponent `p - 2 = 2^255 - 21`, encoded little-endian.
///
/// Fermat's little theorem gives `z^(p-2) = z^-1` for nonzero `z`. The exponent is public and
/// constant; iterating these bits never branches on field contents.
const INVERSE_EXPONENT: [u8; 32] = [
    0xeb, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f,
];

/// One secret-capable field element in little-endian radix-`2^51` form.
///
/// Ladder coordinates become private-key dependent, so this type is non-`Copy`, non-`Clone`,
/// non-formattable, and zeroizing even though the initial input coordinate is public.
pub(super) struct FieldElement {
    limbs: [u64; LIMB_COUNT],
}

impl FieldElement {
    /// The field element zero.
    pub(super) const ZERO: Self = Self {
        limbs: [0; LIMB_COUNT],
    };

    /// The field element one.
    pub(super) const ONE: Self = Self {
        limbs: [1, 0, 0, 0, 0],
    };

    /// Decode one RFC 7748 X25519 u-coordinate.
    ///
    /// **Standard mapping:** §5 interprets bytes in little-endian order and ignores bit 255.
    /// Masking the final radix limb implements that rule. Keeping values up to `2^255 - 1`
    /// initially, then reducing during arithmetic or serialization, accepts all required
    /// non-canonical encodings instead of rejecting them at the byte boundary.
    #[must_use]
    pub(super) fn from_bytes(bytes: &[u8; 32]) -> Self {
        let word_0 = load_word(bytes, 0);
        let word_1 = load_word(bytes, 8);
        let word_2 = load_word(bytes, 16);
        let word_3 = load_word(bytes, 24);

        Self {
            limbs: [
                word_0 & LIMB_MASK,
                ((word_0 >> 51) | (word_1 << 13)) & LIMB_MASK,
                ((word_1 >> 38) | (word_2 << 26)) & LIMB_MASK,
                ((word_2 >> 25) | (word_3 << 39)) & LIMB_MASK,
                (word_3 >> 12) & LIMB_MASK,
            ],
        }
    }

    /// Construct a small public field constant.
    #[must_use]
    #[cfg(test)]
    pub(super) const fn from_u64(value: u64) -> Self {
        Self {
            limbs: [value, 0, 0, 0, 0],
        }
    }

    /// Add two field elements modulo `p`.
    #[must_use]
    pub(super) fn add(&self, right: &Self) -> Self {
        let coefficients = core::array::from_fn(|index| {
            u128::from(self.limbs[index]) + u128::from(right.limbs[index])
        });

        Self::from_coefficients(coefficients)
    }

    /// Subtract one field element from another modulo `p` without unsigned underflow.
    ///
    /// Adding `2p` before subtraction does not change the field value and ensures each
    /// coefficient is nonnegative for the bounded limb representation.
    #[must_use]
    pub(super) fn subtract(&self, right: &Self) -> Self {
        let coefficients = core::array::from_fn(|index| {
            u128::from(self.limbs[index]) + 2 * u128::from(MODULUS_LIMBS[index])
                - u128::from(right.limbs[index])
        });

        Self::from_coefficients(coefficients)
    }

    /// Multiply two field elements and reduce modulo `p`.
    ///
    /// Terms whose radix exponent is five or greater wrap by `2^255 = 19 (mod p)`. Every
    /// coefficient fits in `u128`: at most five products of 51-bit limbs are accumulated, with
    /// wrapped terms multiplied by nineteen.
    #[must_use]
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

    /// Square one field element using the same visibly correct multiplication path.
    #[must_use]
    pub(super) fn square(&self) -> Self {
        self.multiply(self)
    }

    /// Multiply by a small public constant.
    #[must_use]
    pub(super) fn multiply_small(&self, value: u64) -> Self {
        let coefficients = self.limbs.map(|limb| u128::from(limb) * u128::from(value));
        Self::from_coefficients(coefficients)
    }

    /// Calculate `self^(p-2)` through a fixed public square-and-multiply schedule.
    ///
    /// RFC 7748 §5 returns `x_2 * z_2^(p-2)`. All 255 loop iterations execute regardless of the
    /// field value. The only conditional depends on the fixed public exponent, never on secret
    /// scalar or coordinate data.
    #[must_use]
    pub(super) fn invert(&self) -> Self {
        let mut result = Self::ONE;

        for bit_index in (0..255).rev() {
            result = result.square();
            let exponent_bit = (INVERSE_EXPONENT[bit_index / 8] >> (bit_index % 8)) & 1_u8;
            if exponent_bit == 1 {
                result = result.multiply(self);
            }
        }

        result
    }

    /// Encode the unique canonical little-endian representative in `[0, p)`.
    #[must_use]
    pub(super) fn to_bytes(&self) -> [u8; 32] {
        let mut limbs = canonical_limbs(self.limbs);
        let mut words = [
            limbs[0] | (limbs[1] << 51),
            (limbs[1] >> 13) | (limbs[2] << 38),
            (limbs[2] >> 26) | (limbs[3] << 25),
            (limbs[3] >> 39) | (limbs[4] << 12),
        ];
        let mut output = [0_u8; 32];

        for (word_index, word) in words.iter().copied().enumerate() {
            let first_byte = word_index * 8;
            output[first_byte..first_byte + 8].copy_from_slice(&word.to_le_bytes());
        }

        limbs.zeroize();
        words.zeroize();

        output
    }

    /// Swap two field elements using the exact mask construction from RFC 7748 §5.
    ///
    /// `swap` must be zero or one. `0 - swap` becomes either an all-zero or all-one word, so the
    /// instruction sequence and memory access pattern do not depend on the scalar bit.
    pub(super) fn conditional_swap(swap: u64, left: &mut Self, right: &mut Self) {
        let mask = 0_u64.wrapping_sub(swap);

        for limb_index in 0..LIMB_COUNT {
            let selected_difference = mask & (left.limbs[limb_index] ^ right.limbs[limb_index]);
            left.limbs[limb_index] ^= selected_difference;
            right.limbs[limb_index] ^= selected_difference;
        }
    }

    /// Carry and reduce a coefficient array into the owned limb representation.
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

/// Load one explicitly little-endian 64-bit word without host-endian reinterpretation.
#[must_use]
fn load_word(bytes: &[u8; 32], first_byte: usize) -> u64 {
    let word = <[u8; 8]>::try_from(&bytes[first_byte..first_byte + 8])
        .expect("all X25519 word offsets select eight bytes");
    u64::from_le_bytes(word)
}

/// Propagate radix carries and fold every `2^255` multiple back as nineteen.
#[must_use]
fn reduce_coefficients(mut coefficients: [u128; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mask = u128::from(LIMB_MASK);

    // Three fixed passes are sufficient for the documented operation bounds. The first reduces
    // arbitrary multiplication coefficients, the second propagates the folded high carry, and
    // the third resolves the possible one-limb ripple from that propagation.
    for _ in 0..3 {
        for limb_index in 0..LIMB_COUNT - 1 {
            let carry = coefficients[limb_index] >> LIMB_BITS;
            coefficients[limb_index] &= mask;
            coefficients[limb_index + 1] += carry;
        }

        let high_carry = coefficients[4] >> LIMB_BITS;
        coefficients[4] &= mask;
        coefficients[0] += high_carry * 19;
    }

    let output = coefficients.map(|coefficient| {
        u64::try_from(coefficient).expect("three carry passes leave every coefficient below 2^51")
    });
    coefficients.zeroize();
    output
}

/// Conditionally subtract `p` so the returned radix limbs encode a value below the modulus.
#[must_use]
fn canonical_limbs(limbs: [u64; LIMB_COUNT]) -> [u64; LIMB_COUNT] {
    let mut limbs = reduce_coefficients(limbs.map(u128::from));
    let mut difference = [0_u64; LIMB_COUNT];
    let mut borrow = 0_u64;

    for limb_index in 0..LIMB_COUNT {
        let subtrahend = MODULUS_LIMBS[limb_index] + borrow;
        let tentative = limbs[limb_index] + RADIX - subtrahend;
        difference[limb_index] = tentative & LIMB_MASK;
        borrow = 1 - (tentative >> LIMB_BITS);
    }

    // Final borrow zero means `limbs >= p`, so select the difference. Final borrow one means the
    // original value was already canonical. Arithmetic selection avoids a value-dependent branch.
    let use_difference = 1_u64.wrapping_sub(borrow);
    let selection_mask = 0_u64.wrapping_sub(use_difference);
    let output = core::array::from_fn(|index| {
        (limbs[index] & !selection_mask) | (difference[index] & selection_mask)
    });
    limbs.zeroize();
    difference.zeroize();
    output
}

#[cfg(test)]
mod unit {
    use super::FieldElement;

    #[test]
    fn little_endian_encoding_round_trips_the_base_coordinate() {
        let mut bytes = [0_u8; 32];
        bytes[0] = 9;

        assert_eq!(FieldElement::from_bytes(&bytes).to_bytes(), bytes);
    }

    #[test]
    fn high_input_bit_is_ignored_as_required_by_rfc_7748() {
        let mut ordinary = [0_u8; 32];
        ordinary[0] = 9;
        let mut high_bit_set = ordinary;
        high_bit_set[31] = 0x80;

        assert_eq!(
            FieldElement::from_bytes(&ordinary).to_bytes(),
            FieldElement::from_bytes(&high_bit_set).to_bytes()
        );
    }

    #[test]
    fn noncanonical_modulus_and_modulus_plus_nine_reduce_correctly() {
        let mut modulus = [0xff_u8; 32];
        modulus[0] = 0xed;
        modulus[31] = 0x7f;
        let mut modulus_plus_nine = modulus;
        modulus_plus_nine[0] = 0xf6;
        let mut nine = [0_u8; 32];
        nine[0] = 9;

        assert_eq!(FieldElement::from_bytes(&modulus).to_bytes(), [0_u8; 32]);
        assert_eq!(
            FieldElement::from_bytes(&modulus_plus_nine).to_bytes(),
            nine
        );
    }

    #[test]
    fn small_addition_subtraction_and_multiplication_have_expected_residues() {
        let five = FieldElement::from_u64(5);
        let seven = FieldElement::from_u64(7);

        assert_eq!(five.add(&seven).to_bytes()[0], 12);
        assert_eq!(seven.subtract(&five).to_bytes()[0], 2);
        assert_eq!(five.multiply(&seven).to_bytes()[0], 35);
        assert_eq!(seven.square().to_bytes()[0], 49);
    }

    #[test]
    fn inversion_multiplies_back_to_one() {
        let nine = FieldElement::from_u64(9);
        let inverse = nine.invert();

        assert_eq!(
            nine.multiply(&inverse).to_bytes(),
            FieldElement::ONE.to_bytes()
        );
    }

    #[test]
    fn varied_field_values_obey_cancellation_distributivity_and_inversion() {
        for case in 0_u8..64 {
            let left_bytes = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every field byte index fits in u8");
                case.wrapping_mul(0x53)
                    .wrapping_add(index.wrapping_mul(0x1d))
                    .wrapping_add(1)
            });
            let right_bytes = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every field byte index fits in u8");
                case.wrapping_mul(0x71)
                    .wrapping_add(index.wrapping_mul(0x2f))
                    .wrapping_add(0x24)
            });
            let third_bytes = core::array::from_fn(|index| {
                let index = u8::try_from(index).expect("every field byte index fits in u8");
                case.wrapping_mul(0x3d)
                    .wrapping_add(index.wrapping_mul(0x17))
                    .wrapping_add(0x5a)
            });
            let left = FieldElement::from_bytes(&left_bytes);
            let right = FieldElement::from_bytes(&right_bytes);
            let third = FieldElement::from_bytes(&third_bytes);

            assert_eq!(
                left.add(&right).subtract(&right).to_bytes(),
                left.to_bytes()
            );
            assert_eq!(
                left.multiply(&right).to_bytes(),
                right.multiply(&left).to_bytes()
            );
            assert_eq!(
                left.multiply(&right.add(&third)).to_bytes(),
                left.multiply(&right).add(&left.multiply(&third)).to_bytes()
            );

            if left.to_bytes() != [0_u8; 32] {
                assert_eq!(
                    left.multiply(&left.invert()).to_bytes(),
                    FieldElement::ONE.to_bytes()
                );
            }
        }
    }

    #[test]
    fn conditional_swap_obeys_both_public_control_values() {
        let mut left = FieldElement::from_u64(5);
        let mut right = FieldElement::from_u64(7);
        FieldElement::conditional_swap(0, &mut left, &mut right);
        assert_eq!(left.to_bytes()[0], 5);
        assert_eq!(right.to_bytes()[0], 7);

        FieldElement::conditional_swap(1, &mut left, &mut right);
        assert_eq!(left.to_bytes()[0], 7);
        assert_eq!(right.to_bytes()[0], 5);
    }
}
