//! Minimal unsigned-integer and Montgomery arithmetic used by the RSA primitive.
//!
//! RFC 8017 defines RSA over nonnegative integers but deliberately leaves arbitrary-precision
//! arithmetic to the implementation. Limbs here are base `2^32`, least-significant first. The
//! representation is normalized after every operation: zero has no limbs and every nonzero value
//! has a nonzero final limb.
//!
//! Montgomery multiplication represents `x` as `xR mod n`, where `R = 2^(32L)` and `L` is the
//! modulus limb count. One reduction then replaces division by the odd modulus with limb-local
//! multiplication and carry propagation. This is still variable-time teaching code, not a
//! side-channel-hardened bignum implementation: it is adequate for public-key operations such as
//! signature verification, whose inputs are public, and only educational for private-key use.

use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use zeroize::Zeroize;

use crate::{CryptoError, Result};

const LIMB_BITS: usize = u32::BITS as usize;
const LIMB_MASK: u128 = u32::MAX as u128;

/// A normalized, unsigned, little-endian base-`2^32` integer.
pub(crate) struct BigUint {
    limbs: Vec<u32>,
}

impl BigUint {
    pub(crate) fn from_be_bytes(bytes: &[u8]) -> Self {
        let first_nonzero = bytes
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(bytes.len());
        let significant = &bytes[first_nonzero..];
        let mut limbs = Vec::with_capacity(significant.len().div_ceil(4));

        for chunk in significant.rchunks(4) {
            let mut encoded = [0_u8; 4];
            encoded[4 - chunk.len()..].copy_from_slice(chunk);
            limbs.push(u32::from_be_bytes(encoded));
        }

        Self { limbs }
    }

    pub(crate) fn one() -> Self {
        Self { limbs: vec![1] }
    }

    pub(crate) fn bit_len(&self) -> usize {
        self.limbs.last().map_or(0, |last| {
            (self.limbs.len() - 1) * LIMB_BITS
                + usize::try_from(u32::BITS - last.leading_zeros())
                    .expect("a u32 bit count fits usize")
        })
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.bit_len().div_ceil(8)
    }

    pub(crate) fn bit(&self, index: usize) -> bool {
        let limb_index = index / LIMB_BITS;
        let bit_index = index % LIMB_BITS;
        self.limbs
            .get(limb_index)
            .is_some_and(|limb| (limb >> bit_index) & 1 == 1)
    }

    pub(crate) fn is_odd(&self) -> bool {
        self.limbs.first().is_some_and(|limb| limb & 1 == 1)
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub(crate) fn is_one(&self) -> bool {
        self.limbs.as_slice() == [1]
    }

    pub(crate) fn to_be_bytes_padded(&self, output_len: usize) -> Option<Vec<u8>> {
        if self.byte_len() > output_len {
            return None;
        }

        let mut output = vec![0_u8; output_len];
        for (limb_index, limb) in self.limbs.iter().copied().enumerate() {
            let bytes = limb.to_le_bytes();
            for (byte_index, byte) in bytes.into_iter().enumerate() {
                let offset_from_end = limb_index * 4 + byte_index;
                if offset_from_end < output_len {
                    output[output_len - 1 - offset_from_end] = byte;
                }
            }
        }
        Some(output)
    }

    fn limb(&self, index: usize) -> u32 {
        self.limbs.get(index).copied().unwrap_or(0)
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn shift_left_one(&mut self) {
        let mut carry = 0_u32;
        for limb in &mut self.limbs {
            let next_carry = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = next_carry;
        }
        if carry != 0 {
            self.limbs.push(carry);
        }
    }

    fn subtract_assign(&mut self, other: &Self) {
        debug_assert_ne!(self.compare(other), Ordering::Less);
        let mut borrow = false;

        for index in 0..self.limbs.len() {
            let (without_other, borrowed_other) =
                self.limbs[index].overflowing_sub(other.limb(index));
            let (difference, borrowed_carry) = without_other.overflowing_sub(u32::from(borrow));
            self.limbs[index] = difference;
            borrow = borrowed_other || borrowed_carry;
        }

        debug_assert!(!borrow);
        self.normalize();
    }

    fn double_mod(&mut self, modulus: &Self) {
        debug_assert_eq!(self.compare(modulus), Ordering::Less);
        self.shift_left_one();
        if self.compare(modulus) != Ordering::Less {
            self.subtract_assign(modulus);
        }
    }

    pub(crate) fn compare(&self, other: &Self) -> Ordering {
        match self.limbs.len().cmp(&other.limbs.len()) {
            Ordering::Equal => self.limbs.iter().rev().cmp(other.limbs.iter().rev()),
            ordering => ordering,
        }
    }
}

impl Drop for BigUint {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

/// Montgomery context for one odd modulus.
struct Montgomery<'a> {
    modulus: &'a BigUint,
    limb_len: usize,
    negative_inverse: u32,
    r_squared: BigUint,
}

impl<'a> Montgomery<'a> {
    fn new(modulus: &'a BigUint) -> Self {
        debug_assert!(modulus.is_odd());
        let limb_len = modulus.limbs.len();
        let low_limb = modulus.limb(0);

        // Newton iteration doubles the number of correct low inverse bits each time. Starting
        // with one correct bit for an odd number, five iterations cover all 32 bits.
        let mut inverse = 1_u32;
        for _ in 0..5 {
            inverse = inverse.wrapping_mul(2_u32.wrapping_sub(low_limb.wrapping_mul(inverse)));
        }

        // Repeated modular doubling makes 2^(64L) mod n = R^2 mod n. The deliberately plain loop
        // is easier to audit than a second division/remainder implementation.
        let mut r_squared = BigUint::one();
        for _ in 0..(2 * LIMB_BITS * limb_len) {
            r_squared.double_mod(modulus);
        }

        Self {
            modulus,
            limb_len,
            negative_inverse: inverse.wrapping_neg(),
            r_squared,
        }
    }

    fn multiply(&self, left: &BigUint, right: &BigUint) -> BigUint {
        let width = self.limb_len;
        let mut product = vec![0_u32; width * 2 + 2];

        // Schoolbook multiplication produces the 2L-limb integer T = left * right.
        for left_index in 0..width {
            let mut carry = 0_u128;
            for right_index in 0..width {
                let index = left_index + right_index;
                let total = u128::from(product[index])
                    + u128::from(left.limb(left_index)) * u128::from(right.limb(right_index))
                    + carry;
                product[index] = (total & LIMB_MASK) as u32;
                carry = total >> LIMB_BITS;
            }
            add_carry(&mut product, left_index + width, carry);
        }

        // REDC chooses m so each current low limb becomes zero modulo 2^32. Dividing the final
        // T by R is then just taking its upper limbs.
        for reduction_index in 0..width {
            let multiplier = product[reduction_index].wrapping_mul(self.negative_inverse);
            let mut carry = 0_u128;
            for modulus_index in 0..width {
                let index = reduction_index + modulus_index;
                let total = u128::from(product[index])
                    + u128::from(multiplier) * u128::from(self.modulus.limb(modulus_index))
                    + carry;
                product[index] = (total & LIMB_MASK) as u32;
                carry = total >> LIMB_BITS;
            }
            add_carry(&mut product, reduction_index + width, carry);
        }

        let mut result = BigUint {
            limbs: product[width..].to_vec(),
        };
        product.zeroize();
        result.normalize();
        if result.compare(self.modulus) != Ordering::Less {
            result.subtract_assign(self.modulus);
        }
        result
    }

    fn to_montgomery(&self, value: &BigUint) -> BigUint {
        self.multiply(value, &self.r_squared)
    }

    fn decode_montgomery(&self, value: &BigUint) -> BigUint {
        self.multiply(value, &BigUint::one())
    }
}

fn add_carry(words: &mut [u32], mut index: usize, mut carry: u128) {
    while carry != 0 {
        let total = u128::from(words[index]) + carry;
        words[index] = (total & LIMB_MASK) as u32;
        carry = total >> LIMB_BITS;
        index += 1;
    }
}

/// Compute `base^exponent mod modulus` with left-to-right Montgomery exponentiation.
pub(crate) fn modpow(base: &BigUint, exponent: &BigUint, modulus: &BigUint) -> Result<BigUint> {
    if modulus.is_zero() || !modulus.is_odd() || base.compare(modulus) != Ordering::Less {
        return Err(CryptoError::InvalidKey);
    }

    let context = Montgomery::new(modulus);
    let montgomery_base = context.to_montgomery(base);
    let mut accumulator = context.to_montgomery(&BigUint::one());

    for bit_index in (0..exponent.bit_len()).rev() {
        accumulator = context.multiply(&accumulator, &accumulator);
        if exponent.bit(bit_index) {
            accumulator = context.multiply(&accumulator, &montgomery_base);
        }
    }

    Ok(context.decode_montgomery(&accumulator))
}

#[cfg(test)]
mod unit {
    use super::*;
    use num_bigint_dig::BigUint as OracleBigUint;

    fn oracle_bytes(value: &BigUint) -> Vec<u8> {
        value
            .to_be_bytes_padded(value.byte_len())
            .expect("the exact byte length always fits")
    }

    #[test]
    fn byte_conversion_preserves_leading_zero_semantics() {
        let value = BigUint::from_be_bytes(&[0, 0, 0x12, 0x34, 0x56, 0x78, 0x9a]);
        assert_eq!(value.bit_len(), 37);
        assert_eq!(
            value.to_be_bytes_padded(8),
            Some(vec![0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x9a])
        );
        assert_eq!(value.to_be_bytes_padded(4), None);
    }

    #[test]
    fn montgomery_modpow_matches_independent_bigint_across_boundaries() {
        for width in [1_usize, 2, 3, 4, 8, 16] {
            for variation in 0_u8..12 {
                let mut modulus_bytes = vec![0_u8; width * 4];
                for (index, byte) in modulus_bytes.iter_mut().enumerate() {
                    *byte = variation
                        .wrapping_mul(29)
                        .wrapping_add(u8::try_from(index).unwrap_or(u8::MAX).wrapping_mul(17));
                }
                modulus_bytes[0] |= 0x80;
                let last = modulus_bytes.len() - 1;
                modulus_bytes[last] |= 1;

                let modulus = BigUint::from_be_bytes(&modulus_bytes);
                let mut base_bytes = modulus_bytes.clone();
                base_bytes[0] &= 0x3f;
                base_bytes[last] &= 0xfe;
                let base = BigUint::from_be_bytes(&base_bytes);
                let exponent_bytes = [0x01, variation.wrapping_add(1), 0x01];
                let exponent = BigUint::from_be_bytes(&exponent_bytes);

                let actual = modpow(&base, &exponent, &modulus).expect("the modulus is odd");
                let oracle_modulus = OracleBigUint::from_bytes_be(&modulus_bytes);
                let oracle_base = OracleBigUint::from_bytes_be(&base_bytes);
                let oracle_exponent = OracleBigUint::from_bytes_be(&exponent_bytes);
                let expected = oracle_base
                    .modpow(&oracle_exponent, &oracle_modulus)
                    .to_bytes_be();

                assert_eq!(
                    oracle_bytes(&actual),
                    expected,
                    "width {width}, variation {variation}"
                );
            }
        }
    }
}
