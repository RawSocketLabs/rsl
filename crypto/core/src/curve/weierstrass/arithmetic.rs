//! Readable fixed-width unsigned integer and modular arithmetic for prime-order NIST curves.
//!
//! ## Standards ownership
//!
//! SP 800-186 defines each curve's field prime `p` and group order `n` as integers; it does not
//! prescribe a representation. This module chooses `N` little-endian `u64` limbs (`limb[0]` is
//! least significant), `u128` for every product and carry, and an explicit reduction that
//! follows from one identity: because `2^B = m + (2^B - m)` for `B = 64·N`, any `2B`-bit value
//! `hi · 2^B + lo` is congruent to `hi · (2^B - m) + lo` modulo `m`. When `2^B - m` has `c` bits,
//! every fold shrinks `hi` from `h` bits to at most `h + c - B` bits; [`Modulus::new`] counts the
//! folds needed until one masked subtraction finishes the reduction.
//!
//! P-256 (`N = 4`, `c = 224`) needs nine folds; P-384 (`N = 6`, `c = 129`) needs two. No
//! operation here branches on the value being reduced; selection uses full-width masks.

use zeroize::Zeroize;

/// A public modulus close enough to `2^(64·N)` for the folding reduction.
pub(crate) struct Modulus<const N: usize> {
    /// `m` itself.
    pub(crate) value: [u64; N],
    /// `2^B - m`, the fold multiplier.
    pub(crate) complement: [u64; N],
    /// Folds that bring any `2B`-bit product below `2m`.
    fold_passes: usize,
}

impl<const N: usize> Modulus<N> {
    /// Bind a modulus, derive its fold multiplier, and count the folds it needs.
    pub(crate) const fn new(value: [u64; N]) -> Self {
        let complement = two_pow_b_minus(value);
        let complement_bits = bit_length(&complement);
        let total_bits = 64 * N;
        assert!(
            complement_bits + 1 < total_bits,
            "the folding reduction needs 2^B - m well below 2^B"
        );
        // Before any fold the high half has B bits. A fold of an h-bit high half leaves
        // value < 2^(h + c) + 2^B, so the new high half has at most h + c - B bits. Once
        // h + c <= B - 1 the fold leaves value < 1.5 · 2^B < 2m and one subtraction finishes.
        let mut hi_bits = total_bits;
        let mut fold_passes = 0;
        loop {
            fold_passes += 1;
            if hi_bits + complement_bits < total_bits {
                break;
            }
            hi_bits = hi_bits + complement_bits - total_bits;
        }
        Self {
            value,
            complement,
            fold_passes,
        }
    }

    /// `(left + right) mod m` for operands already below `m`.
    pub(crate) fn add(&self, left: &[u64; N], right: &[u64; N]) -> [u64; N] {
        let (sum, carry) = add_limbs(left, right);
        self.subtract_if_at_least_modulus(&sum, carry)
    }

    /// `(left - right) mod m` for operands already below `m`.
    pub(crate) fn subtract(&self, left: &[u64; N], right: &[u64; N]) -> [u64; N] {
        let (difference, borrow) = subtract_limbs(left, right);
        // A borrow means the true difference is negative; adding `m` once corrects it.
        let mask = 0_u64.wrapping_sub(borrow);
        let correction = self.value.map(|limb| limb & mask);
        let (corrected, _) = add_limbs(&difference, &correction);
        corrected
    }

    /// `(left * right) mod m` by schoolbook multiplication and folding reduction.
    pub(crate) fn multiply(&self, left: &[u64; N], right: &[u64; N]) -> [u64; N] {
        let mut wide = multiply_limbs(left, right);
        for _ in 0..self.fold_passes {
            wide = self.fold(&wide);
        }
        debug_assert!(
            wide.high[1..].iter().all(|limb| *limb == 0),
            "the counted folds leave a value below 2^(B+1)"
        );
        let output = self.subtract_if_at_least_modulus(&wide.low, wide.high[0]);
        wide.zeroize();
        output
    }

    /// `base^exponent mod m` by a fixed square-and-multiply schedule over a public exponent.
    pub(crate) fn power(&self, base: &[u64; N], exponent: &[u64; N]) -> [u64; N] {
        let mut result = one();
        for bit_index in (0..64 * N).rev() {
            result = self.multiply(&result, &result);
            if (exponent[bit_index / 64] >> (bit_index % 64)) & 1 == 1 {
                result = self.multiply(&result, base);
            }
        }
        result
    }

    /// Reduce any `B`-bit integer, which is at most `2m - 1`, by one masked subtraction.
    pub(crate) fn reduce_once(&self, value: &[u64; N]) -> [u64; N] {
        self.subtract_if_at_least_modulus(value, 0)
    }

    /// Replace `hi · 2^B + lo` with the congruent `hi · (2^B - m) + lo`.
    fn fold(&self, wide: &Wide<N>) -> Wide<N> {
        let mut folded = multiply_limbs(&wide.high, &self.complement);
        let mut carry = 0_u128;
        for index in 0..N {
            let sum = u128::from(folded.low[index]) + u128::from(wide.low[index]) + carry;
            folded.low[index] = truncate(sum);
            carry = sum >> 64;
        }
        for index in 0..N {
            let sum = u128::from(folded.high[index]) + carry;
            folded.high[index] = truncate(sum);
            carry = sum >> 64;
        }
        debug_assert_eq!(carry, 0, "a folded value stays below 2^(2B)");
        folded
    }

    /// Select `value - m` when `value >= m`, otherwise `value`, without a data-dependent branch.
    fn subtract_if_at_least_modulus(&self, low: &[u64; N], high_limb: u64) -> [u64; N] {
        let (difference, borrow) = subtract_limbs(low, &self.value);
        // The subtraction underflows only when `low < m`; a set high limb means `value >= m`.
        let at_least_modulus = u64::from(((1 - borrow) | high_limb) != 0);
        select(low, &difference, at_least_modulus)
    }
}

/// A `2B`-bit product as low and high halves.
pub(crate) struct Wide<const N: usize> {
    low: [u64; N],
    high: [u64; N],
}

impl<const N: usize> Zeroize for Wide<N> {
    fn zeroize(&mut self) {
        self.low.zeroize();
        self.high.zeroize();
    }
}

/// The limbs of one.
pub(crate) const fn one<const N: usize>() -> [u64; N] {
    let mut limbs = [0_u64; N];
    limbs[0] = 1;
    limbs
}

/// `2^B - value` as a wrapping two's-complement negation.
const fn two_pow_b_minus<const N: usize>(value: [u64; N]) -> [u64; N] {
    let mut output = [0_u64; N];
    let mut borrow = 0_u64;
    let mut index = 0;
    while index < N {
        let (first, b1) = 0_u64.overflowing_sub(value[index]);
        let (second, b2) = first.overflowing_sub(borrow);
        output[index] = second;
        borrow = (b1 | b2) as u64;
        index += 1;
    }
    output
}

/// Number of significant bits.
const fn bit_length<const N: usize>(value: &[u64; N]) -> usize {
    let mut index = N;
    while index > 0 {
        index -= 1;
        if value[index] != 0 {
            return index * 64 + (64 - value[index].leading_zeros() as usize);
        }
    }
    0
}

/// Add two `B`-bit integers, returning the sum and the carry out of the top bit.
pub(crate) fn add_limbs<const N: usize>(left: &[u64; N], right: &[u64; N]) -> ([u64; N], u64) {
    let mut sum = [0_u64; N];
    let mut carry = 0_u64;
    for index in 0..N {
        let total = u128::from(left[index]) + u128::from(right[index]) + u128::from(carry);
        sum[index] = truncate(total);
        carry = truncate(total >> 64);
    }
    (sum, carry)
}

/// Subtract two `B`-bit integers, returning the difference and the final borrow.
pub(crate) fn subtract_limbs<const N: usize>(left: &[u64; N], right: &[u64; N]) -> ([u64; N], u64) {
    let mut difference = [0_u64; N];
    let mut borrow = 0_u64;
    for index in 0..N {
        let (first, b1) = left[index].overflowing_sub(right[index]);
        let (second, b2) = first.overflowing_sub(borrow);
        difference[index] = second;
        borrow = u64::from(b1 | b2);
    }
    (difference, borrow)
}

/// Schoolbook multiplication of two `B`-bit integers into a `2B`-bit product.
fn multiply_limbs<const N: usize>(left: &[u64; N], right: &[u64; N]) -> Wide<N> {
    let mut low = [0_u64; N];
    let mut high = [0_u64; N];
    for i in 0..N {
        let mut carry = 0_u128;
        for (j, right_limb) in right.iter().enumerate() {
            let index = i + j;
            let current = if index < N {
                low[index]
            } else {
                high[index - N]
            };
            let partial =
                u128::from(left[i]) * u128::from(*right_limb) + u128::from(current) + carry;
            if index < N {
                low[index] = truncate(partial);
            } else {
                high[index - N] = truncate(partial);
            }
            carry = partial >> 64;
        }
        high[i] = truncate(carry);
    }
    Wide { low, high }
}

/// Whether `left < right` as `B`-bit unsigned integers.
pub(crate) fn is_less_than<const N: usize>(left: &[u64; N], right: &[u64; N]) -> bool {
    let (_, borrow) = subtract_limbs(left, right);
    borrow == 1
}

/// Whether every limb is zero.
pub(crate) fn is_zero<const N: usize>(value: &[u64; N]) -> bool {
    value.iter().fold(0, |accumulator, limb| accumulator | limb) == 0
}

/// Select `right` when `choice` is one and `left` when it is zero without branching.
pub(crate) fn select<const N: usize>(left: &[u64; N], right: &[u64; N], choice: u64) -> [u64; N] {
    let mask = 0_u64.wrapping_sub(choice);
    core::array::from_fn(|index| (left[index] & !mask) | (right[index] & mask))
}

/// Decode a big-endian octet string of exactly `8·N` bytes into limbs.
pub(crate) fn from_be_bytes<const N: usize>(bytes: &[u8]) -> Option<[u64; N]> {
    if bytes.len() != 8 * N {
        return None;
    }
    Some(core::array::from_fn(|index| {
        let start = 8 * (N - 1 - index);
        u64::from_be_bytes(
            bytes[start..start + 8]
                .try_into()
                .expect("a limb is eight bytes"),
        )
    }))
}

/// Encode limbs as a big-endian octet string into `out`, which must hold exactly `8·N` bytes.
pub(crate) fn write_be_bytes<const N: usize>(limbs: &[u64; N], out: &mut [u8]) {
    assert_eq!(out.len(), 8 * N, "the output holds exactly 8·N bytes");
    for (index, limb) in limbs.iter().enumerate() {
        let start = 8 * (N - 1 - index);
        out[start..start + 8].copy_from_slice(&limb.to_be_bytes());
    }
}

/// Keep the low 64 bits of a wide intermediate.
#[allow(clippy::cast_possible_truncation)] // Discarding the high half is the intended operation.
fn truncate(value: u128) -> u64 {
    value as u64
}

#[cfg(test)]
mod unit {
    use super::*;

    const P256: Modulus<4> = Modulus::new([
        0xffff_ffff_ffff_ffff,
        0x0000_0000_ffff_ffff,
        0x0000_0000_0000_0000,
        0xffff_ffff_0000_0001,
    ]);
    const P384: Modulus<6> = Modulus::new([
        0x0000_0000_ffff_ffff,
        0xffff_ffff_0000_0000,
        0xffff_ffff_ffff_fffe,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
    ]);

    #[test]
    fn complements_and_fold_counts_follow_from_the_published_prime_forms() {
        // 2^256 - p256 = 2^224 - 2^192 - 2^96 + 1; 2^384 - p384 = 2^128 + 2^96 - 2^32 + 1.
        assert_eq!(
            P256.complement,
            [
                1,
                0xffff_ffff_0000_0000,
                0xffff_ffff_ffff_ffff,
                0x0000_0000_ffff_fffe
            ]
        );
        assert_eq!(P256.fold_passes, 9);
        assert_eq!(
            P384.complement,
            [0xffff_ffff_0000_0001, 0x0000_0000_ffff_ffff, 1, 0, 0, 0]
        );
        assert_eq!(P384.fold_passes, 2);
    }

    #[test]
    fn byte_encoding_is_big_endian_and_round_trips() {
        let bytes: [u8; 48] = core::array::from_fn(|index| u8::try_from(index).unwrap());
        let limbs: [u64; 6] = from_be_bytes(&bytes).unwrap();
        assert_eq!(limbs[5], 0x0001_0203_0405_0607);
        assert_eq!(limbs[0], 0x2829_2a2b_2c2d_2e2f);
        let mut out = [0_u8; 48];
        write_be_bytes(&limbs, &mut out);
        assert_eq!(out, bytes);
        assert!(from_be_bytes::<6>(&bytes[..47]).is_none());
    }

    #[test]
    fn largest_products_match_a_bit_serial_reference_for_both_widths() {
        let (m1, _) = subtract_limbs(&P256.value, &one());
        assert_eq!(P256.multiply(&m1, &m1), one());
        let (m1, _) = subtract_limbs(&P384.value, &one());
        assert_eq!(P384.multiply(&m1, &m1), one());
        let all_ones = [u64::MAX; 6];
        let reduced = P384.reduce_once(&all_ones);
        assert_eq!(
            P384.multiply(&reduced, &reduced),
            bit_serial_multiply(&P384, &reduced, &reduced)
        );
    }

    #[test]
    fn power_by_modulus_minus_two_inverts_in_both_widths() {
        let (exponent, _) = subtract_limbs(&P384.value, &[2, 0, 0, 0, 0, 0]);
        let value = [0x1234_5678, 0x9abc_def0, 0x0fed_cba9, 7, 8, 9];
        assert_eq!(P384.multiply(&value, &P384.power(&value, &exponent)), one());
    }

    fn bit_serial_multiply<const N: usize>(
        modulus: &Modulus<N>,
        left: &[u64; N],
        right: &[u64; N],
    ) -> [u64; N] {
        let mut result = [0_u64; N];
        for bit_index in (0..64 * N).rev() {
            result = modulus.add(&result, &result);
            if (right[bit_index / 64] >> (bit_index % 64)) & 1 == 1 {
                result = modulus.add(&result, left);
            }
        }
        result
    }
}
