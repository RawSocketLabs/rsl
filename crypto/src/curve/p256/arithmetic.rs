//! Readable 256-bit unsigned integer and modular arithmetic for P-256.
//!
//! ## Standards ownership
//!
//! SP 800-186 defines P-256's field prime `p` and group order `n` as integers; it does not
//! prescribe a representation. This module chooses four little-endian `u64` limbs (`limb[0]` is
//! least significant), `u128` for every product and carry, and an explicit reduction that
//! follows from one identity: because `2^256 = m + (2^256 - m)`, any 512-bit value
//! `hi * 2^256 + lo` is congruent to `hi * (2^256 - m) + lo` modulo `m`. When `2^256 - m` is
//! below `2^224`, every fold removes at least 31 bits, nine folds leave a value below
//! `2^256 + 2^232 < 2m`, and one masked subtraction finishes the reduction.
//!
//! No operation here branches on the value being reduced; selection uses full-width masks.

use zeroize::Zeroize;

/// A 256-bit unsigned integer as four little-endian 64-bit limbs.
pub(crate) type Limbs = [u64; 4];

/// A 512-bit product as eight little-endian 64-bit limbs.
pub(crate) type WideLimbs = [u64; 8];

/// Folds that reduce any 512-bit product below `2m` when `2^256 - m < 2^224`.
const FOLD_PASSES: usize = 9;

/// A public modulus close enough to `2^256` for the folding reduction.
pub(crate) struct Modulus {
    /// `m` itself.
    pub(crate) value: Limbs,
    /// `2^256 - m`, the fold multiplier.
    pub(crate) complement: Limbs,
}

impl Modulus {
    /// Bind a modulus and derive its fold multiplier `2^256 - m`.
    pub(crate) const fn new(value: Limbs) -> Self {
        let complement = two_pow_256_minus(value);
        assert!(
            complement[3] < (1_u64 << 32),
            "the folding reduction requires 2^256 - m < 2^224"
        );
        Self { value, complement }
    }

    /// `(left + right) mod m` for operands already below `m`.
    pub(crate) fn add(&self, left: &Limbs, right: &Limbs) -> Limbs {
        let (sum, carry) = add_limbs(left, right);
        self.subtract_if_at_least_modulus([sum[0], sum[1], sum[2], sum[3], carry])
    }

    /// `(left - right) mod m` for operands already below `m`.
    pub(crate) fn subtract(&self, left: &Limbs, right: &Limbs) -> Limbs {
        let (difference, borrow) = subtract_limbs(left, right);
        // A borrow means the true difference is negative; adding `m` once corrects it.
        let mask = 0_u64.wrapping_sub(borrow);
        let correction = self.value.map(|limb| limb & mask);
        let (corrected, _) = add_limbs(&difference, &correction);
        corrected
    }

    /// `(left * right) mod m` by schoolbook multiplication and folding reduction.
    pub(crate) fn multiply(&self, left: &Limbs, right: &Limbs) -> Limbs {
        let mut wide = multiply_limbs(left, right);
        for _ in 0..FOLD_PASSES {
            wide = self.fold(wide);
        }
        debug_assert!(
            wide[5] == 0 && wide[6] == 0 && wide[7] == 0,
            "nine folds leave a value below 2^257"
        );
        let output =
            self.subtract_if_at_least_modulus([wide[0], wide[1], wide[2], wide[3], wide[4]]);
        wide.zeroize();
        output
    }

    /// `base^exponent mod m` by a fixed 256-step square-and-multiply schedule.
    ///
    /// The exponent is always a public constant such as `m - 2`, so its bits may select
    /// multiplications without concern for timing.
    pub(crate) fn power(&self, base: &Limbs, exponent: &Limbs) -> Limbs {
        let mut result = [1, 0, 0, 0];
        for bit_index in (0..256).rev() {
            result = self.multiply(&result, &result);
            if (exponent[bit_index / 64] >> (bit_index % 64)) & 1 == 1 {
                result = self.multiply(&result, base);
            }
        }
        result
    }

    /// Reduce any 256-bit integer, which is at most `2m - 1`, by one masked subtraction.
    pub(crate) fn reduce_once(&self, value: &Limbs) -> Limbs {
        self.subtract_if_at_least_modulus([value[0], value[1], value[2], value[3], 0])
    }

    /// Replace `hi * 2^256 + lo` with the congruent `hi * (2^256 - m) + lo`.
    fn fold(&self, wide: WideLimbs) -> WideLimbs {
        let low = [wide[0], wide[1], wide[2], wide[3]];
        let high = [wide[4], wide[5], wide[6], wide[7]];
        let mut folded = multiply_limbs(&high, &self.complement);
        let mut carry = 0_u128;
        for index in 0..8 {
            let addend = if index < 4 { low[index] } else { 0 };
            let sum = u128::from(folded[index]) + u128::from(addend) + carry;
            folded[index] = truncate(sum);
            carry = sum >> 64;
        }
        debug_assert_eq!(carry, 0, "a folded value stays below 2^481");
        folded
    }

    /// Select `value - m` when `value >= m`, otherwise `value`, without a data-dependent branch.
    fn subtract_if_at_least_modulus(&self, value: [u64; 5]) -> Limbs {
        let low = [value[0], value[1], value[2], value[3]];
        let (difference, borrow) = subtract_limbs(&low, &self.value);
        // The subtraction underflows only when `low < m`; a set fifth limb means `value >= m`.
        let at_least_modulus = u64::from(((1 - borrow) | value[4]) != 0);
        select(&low, &difference, at_least_modulus)
    }
}

/// `2^256 - value` as a wrapping two's-complement negation.
const fn two_pow_256_minus(value: Limbs) -> Limbs {
    let mut output = [0_u64; 4];
    let mut borrow = 0_u64;
    let mut index = 0;
    while index < 4 {
        let (first, b1) = 0_u64.overflowing_sub(value[index]);
        let (second, b2) = first.overflowing_sub(borrow);
        output[index] = second;
        borrow = (b1 | b2) as u64;
        index += 1;
    }
    output
}

/// Add two 256-bit integers, returning the sum and the carry out of bit 255.
pub(crate) fn add_limbs(left: &Limbs, right: &Limbs) -> (Limbs, u64) {
    let mut sum = [0_u64; 4];
    let mut carry = 0_u64;
    for index in 0..4 {
        let total = u128::from(left[index]) + u128::from(right[index]) + u128::from(carry);
        sum[index] = truncate(total);
        carry = truncate(total >> 64);
    }
    (sum, carry)
}

/// Subtract two 256-bit integers, returning the difference and the final borrow.
pub(crate) fn subtract_limbs(left: &Limbs, right: &Limbs) -> (Limbs, u64) {
    let mut difference = [0_u64; 4];
    let mut borrow = 0_u64;
    for index in 0..4 {
        let (first, b1) = left[index].overflowing_sub(right[index]);
        let (second, b2) = first.overflowing_sub(borrow);
        difference[index] = second;
        borrow = u64::from(b1 | b2);
    }
    (difference, borrow)
}

/// Schoolbook multiplication of two 256-bit integers into a 512-bit product.
pub(crate) fn multiply_limbs(left: &Limbs, right: &Limbs) -> WideLimbs {
    let mut product = [0_u64; 8];
    for i in 0..4 {
        let mut carry = 0_u128;
        for j in 0..4 {
            let partial =
                u128::from(left[i]) * u128::from(right[j]) + u128::from(product[i + j]) + carry;
            product[i + j] = truncate(partial);
            carry = partial >> 64;
        }
        product[i + 4] = truncate(carry);
    }
    product
}

/// Whether `left < right` as 256-bit unsigned integers.
pub(crate) fn is_less_than(left: &Limbs, right: &Limbs) -> bool {
    let (_, borrow) = subtract_limbs(left, right);
    borrow == 1
}

/// Whether every limb is zero.
pub(crate) fn is_zero(value: &Limbs) -> bool {
    value.iter().fold(0, |accumulator, limb| accumulator | limb) == 0
}

/// Select `right` when `choice` is one and `left` when it is zero without branching.
pub(crate) fn select(left: &Limbs, right: &Limbs, choice: u64) -> Limbs {
    let mask = 0_u64.wrapping_sub(choice);
    core::array::from_fn(|index| (left[index] & !mask) | (right[index] & mask))
}

/// Decode a 32-byte big-endian octet string into limbs.
pub(crate) fn from_be_bytes(bytes: &[u8; 32]) -> Limbs {
    core::array::from_fn(|index| {
        let start = 32 - 8 * (index + 1);
        u64::from_be_bytes(
            bytes[start..start + 8]
                .try_into()
                .expect("a limb is eight bytes"),
        )
    })
}

/// Encode limbs as a 32-byte big-endian octet string.
pub(crate) fn to_be_bytes(limbs: &Limbs) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    for (index, limb) in limbs.iter().enumerate() {
        let start = 32 - 8 * (index + 1);
        bytes[start..start + 8].copy_from_slice(&limb.to_be_bytes());
    }
    bytes
}

/// Keep the low 64 bits of a wide intermediate.
#[allow(clippy::cast_possible_truncation)] // Discarding the high half is the intended operation.
fn truncate(value: u128) -> u64 {
    value as u64
}

#[cfg(test)]
mod unit {
    use super::*;

    const TEST_MODULUS: Modulus = Modulus::new([
        0xffff_ffff_ffff_ffff,
        0x0000_0000_ffff_ffff,
        0x0000_0000_0000_0000,
        0xffff_ffff_0000_0001,
    ]);

    #[test]
    fn complement_of_p256_prime_is_derived_from_the_published_form() {
        // 2^256 - p = 2^224 - 2^192 - 2^96 + 1.
        assert_eq!(
            TEST_MODULUS.complement,
            [
                1,
                0xffff_ffff_0000_0000,
                0xffff_ffff_ffff_ffff,
                0x0000_0000_ffff_fffe
            ]
        );
    }

    #[test]
    fn byte_encoding_is_big_endian_and_round_trips() {
        let bytes: [u8; 32] = core::array::from_fn(|index| u8::try_from(index).unwrap());
        let limbs = from_be_bytes(&bytes);
        assert_eq!(limbs[3], 0x0001_0203_0405_0607);
        assert_eq!(limbs[0], 0x1819_1a1b_1c1d_1e1f);
        assert_eq!(to_be_bytes(&limbs), bytes);
    }

    #[test]
    fn multiplying_modulus_minus_one_by_itself_gives_one() {
        let (minus_one, _) = subtract_limbs(&TEST_MODULUS.value, &[1, 0, 0, 0]);
        assert_eq!(TEST_MODULUS.multiply(&minus_one, &minus_one), [1, 0, 0, 0]);
    }

    #[test]
    fn addition_and_subtraction_wrap_at_the_modulus() {
        let (minus_one, _) = subtract_limbs(&TEST_MODULUS.value, &[1, 0, 0, 0]);
        assert_eq!(TEST_MODULUS.add(&minus_one, &[1, 0, 0, 0]), [0; 4]);
        assert_eq!(TEST_MODULUS.subtract(&[0; 4], &[1, 0, 0, 0]), minus_one);
        assert_eq!(TEST_MODULUS.add(&minus_one, &minus_one), {
            let (m2, _) = subtract_limbs(&minus_one, &[1, 0, 0, 0]);
            m2
        });
    }

    #[test]
    fn reducing_the_largest_product_matches_a_bit_serial_reference() {
        let all_ones = [u64::MAX; 4];
        let reduced_input = TEST_MODULUS.reduce_once(&all_ones);
        let fast = TEST_MODULUS.multiply(&reduced_input, &reduced_input);
        let slow = bit_serial_multiply(&TEST_MODULUS, &reduced_input, &reduced_input);
        assert_eq!(fast, slow);
    }

    #[test]
    fn power_by_modulus_minus_two_inverts() {
        let (exponent, _) = subtract_limbs(&TEST_MODULUS.value, &[2, 0, 0, 0]);
        let value = [0x1234_5678, 0x9abc_def0, 0x0fed_cba9, 0x0000_0001];
        let inverse = TEST_MODULUS.power(&value, &exponent);
        assert_eq!(TEST_MODULUS.multiply(&value, &inverse), [1, 0, 0, 0]);
    }

    /// Double-and-add modular multiplication used only as an independent test oracle.
    fn bit_serial_multiply(modulus: &Modulus, left: &Limbs, right: &Limbs) -> Limbs {
        let mut result = [0_u64; 4];
        for bit_index in (0..256).rev() {
            result = modulus.add(&result, &result);
            if (right[bit_index / 64] >> (bit_index % 64)) & 1 == 1 {
                result = modulus.add(&result, left);
            }
        }
        result
    }
}
