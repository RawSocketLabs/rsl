//! Integers modulo Ed25519's prime subgroup order `L`.
//!
//! RFC 8032 §5.1 defines
//! `L = 2^252 + 27742317777372353535851937790883648493`. Hash outputs are interpreted
//! little-endian and reduced modulo `L`; encoded signature scalar `S` must instead already be in
//! `0..L`. This reference implementation uses four little-endian `u64` limbs and bit-at-a-time
//! modular reduction so every carry and conditional subtraction remains inspectable.

use zeroize::Zeroize;

/// `L` in four little-endian 64-bit limbs.
const ORDER: [u64; 4] = [
    0x5812_631a_5cf5_d3ed,
    0x14de_f9de_a2f7_9cd6,
    0x0000_0000_0000_0000,
    0x1000_0000_0000_0000,
];

/// One secret-capable residue modulo `L`.
pub(super) struct Scalar {
    limbs: [u64; 4],
}

impl Scalar {
    const ZERO: Self = Self { limbs: [0; 4] };

    /// Reduce a 64-byte SHA-512 output interpreted little-endian.
    pub(super) fn reduce_wide(bytes: &[u8; 64]) -> Self {
        Self::reduce_bytes(bytes)
    }

    /// Reduce a 32-byte secret scalar interpreted little-endian.
    pub(super) fn reduce_32(bytes: &[u8; 32]) -> Self {
        Self::reduce_bytes(bytes)
    }

    /// Decode `S`, rejecting the non-canonical values `L..2^256-1` required by §5.1.7.
    pub(super) fn from_canonical_bytes(bytes: &[u8; 32]) -> Option<Self> {
        let limbs = decode(bytes);
        let (_, borrow) = subtract_limbs(limbs, ORDER);
        if borrow == 1 {
            Some(Self { limbs })
        } else {
            None
        }
    }

    /// Add two residues and reduce the sum modulo `L`.
    pub(super) fn add(&self, right: &Self) -> Self {
        let mut sum = [0_u64; 4];
        let mut carry = 0_u64;
        for (index, output) in sum.iter_mut().enumerate() {
            let (first, c1) = self.limbs[index].overflowing_add(right.limbs[index]);
            let (second, c2) = first.overflowing_add(carry);
            *output = second;
            carry = u64::from(c1 | c2);
        }
        debug_assert_eq!(carry, 0, "two residues below L fit in 253 bits");
        Self {
            limbs: subtract_order_if_needed(sum),
        }
    }

    /// Multiply two residues with a fixed 256-step double-and-add schedule.
    pub(super) fn multiply(&self, right: &Self) -> Self {
        let mut result = Self::ZERO;
        let mut addend = Self { limbs: self.limbs };
        for bit_index in 0..256 {
            let candidate = result.add(&addend);
            let bit = (right.limbs[bit_index / 64] >> (bit_index % 64)) & 1;
            result = Self::conditional_select(&result, &candidate, bit);
            addend = addend.add(&addend);
        }
        result
    }

    /// Return the canonical little-endian scalar bytes.
    pub(super) fn to_bytes(&self) -> [u8; 32] {
        let mut output = [0_u8; 32];
        for (index, limb) in self.limbs.iter().enumerate() {
            output[index * 8..index * 8 + 8].copy_from_slice(&limb.to_le_bytes());
        }
        output
    }

    fn reduce_bytes(bytes: &[u8]) -> Self {
        let mut remainder = Self::ZERO;
        for bit_index in (0..bytes.len() * 8).rev() {
            let bit = (bytes[bit_index / 8] >> (bit_index % 8)) & 1;
            remainder = remainder.double_and_add_bit(u64::from(bit));
        }
        remainder
    }

    fn double_and_add_bit(&self, bit: u64) -> Self {
        let mut doubled = [0_u64; 4];
        let mut carry = bit;
        for (index, output) in doubled.iter_mut().enumerate() {
            let next_carry = self.limbs[index] >> 63;
            *output = (self.limbs[index] << 1) | carry;
            carry = next_carry;
        }
        debug_assert_eq!(carry, 0, "twice a residue below L fits in 253 bits");
        Self {
            limbs: subtract_order_if_needed(doubled),
        }
    }

    fn conditional_select(left: &Self, right: &Self, choice: u64) -> Self {
        let mask = 0_u64.wrapping_sub(choice);
        Self {
            limbs: core::array::from_fn(|i| (left.limbs[i] & !mask) | (right.limbs[i] & mask)),
        }
    }
}

impl Drop for Scalar {
    fn drop(&mut self) {
        self.limbs.zeroize();
    }
}

fn decode(bytes: &[u8; 32]) -> [u64; 4] {
    core::array::from_fn(|index| {
        u64::from_le_bytes(
            bytes[index * 8..index * 8 + 8]
                .try_into()
                .expect("scalar limb is eight bytes"),
        )
    })
}

/// Subtract four limbs, returning the difference and final borrow bit.
fn subtract_limbs(left: [u64; 4], right: [u64; 4]) -> ([u64; 4], u64) {
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

/// Select `value-L` exactly when `value >= L`, with no data-dependent branch.
fn subtract_order_if_needed(value: [u64; 4]) -> [u64; 4] {
    let (mut difference, borrow) = subtract_limbs(value, ORDER);
    let mask = 0_u64.wrapping_sub(1 - borrow);
    let output = core::array::from_fn(|i| (value[i] & !mask) | (difference[i] & mask));
    difference.zeroize();
    output
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn canonical_boundary_accepts_l_minus_one_and_rejects_l() {
        let mut order_bytes = [0_u8; 32];
        for (index, limb) in ORDER.iter().enumerate() {
            order_bytes[index * 8..index * 8 + 8].copy_from_slice(&limb.to_le_bytes());
        }
        let mut below = order_bytes;
        below[0] -= 1;
        assert!(Scalar::from_canonical_bytes(&below).is_some());
        assert!(Scalar::from_canonical_bytes(&order_bytes).is_none());
    }

    #[test]
    fn reduction_maps_order_to_zero_and_order_plus_one_to_one() {
        let mut order_bytes = [0_u8; 32];
        for (index, limb) in ORDER.iter().enumerate() {
            order_bytes[index * 8..index * 8 + 8].copy_from_slice(&limb.to_le_bytes());
        }
        assert_eq!(Scalar::reduce_32(&order_bytes).to_bytes(), [0_u8; 32]);
        order_bytes[0] += 1;
        let mut one = [0_u8; 32];
        one[0] = 1;
        assert_eq!(Scalar::reduce_32(&order_bytes).to_bytes(), one);
    }

    #[test]
    fn multiplication_and_addition_match_small_integers() {
        let mut a = [0_u8; 32];
        a[0] = 19;
        let mut b = [0_u8; 32];
        b[0] = 23;
        let left = Scalar::reduce_32(&a);
        let right = Scalar::reduce_32(&b);
        assert_eq!(left.add(&right).to_bytes()[0], 42);
        let product = left.multiply(&right).to_bytes();
        assert_eq!(u16::from_le_bytes([product[0], product[1]]), 437);
    }
}
