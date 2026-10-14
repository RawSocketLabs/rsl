//! Integers modulo Ed448's prime subgroup order `L`.
//!
//! RFC 8032 §5.2 defines `L = 2^446 - 13818066809895115352007386748515426880336692474882178609894547503885`.
//! SHAKE256 outputs of 114 bytes are interpreted little-endian and reduced modulo `L`; encoded
//! signature scalar `S` must already be in `0..L`. Eight little-endian `u64` limbs and
//! bit-at-a-time modular reduction keep every carry and conditional subtraction inspectable.

use zeroize::Zeroize;

const LIMBS: usize = 8;

/// `L` as 57 little-endian bytes (the ten most significant bits are zero).
const ORDER_BYTES: [u8; 57] = [
    0xf3, 0x44, 0x58, 0xab, 0x92, 0xc2, 0x78, 0x23, 0x55, 0x8f, 0xc5, 0x8d, 0x72, 0xc2, 0x6c, 0x21,
    0x90, 0x36, 0xd6, 0xae, 0x49, 0xdb, 0x4e, 0xc4, 0xe9, 0x23, 0xca, 0x7c, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x3f, 0x00,
];

fn order_limbs() -> [u64; LIMBS] {
    let mut padded = [0_u8; 64];
    padded[..57].copy_from_slice(&ORDER_BYTES);
    core::array::from_fn(|i| u64::from_le_bytes(padded[i * 8..i * 8 + 8].try_into().unwrap()))
}

/// One secret-capable residue modulo `L`.
pub(super) struct Scalar {
    limbs: [u64; LIMBS],
}

impl Scalar {
    const ZERO: Self = Self { limbs: [0; LIMBS] };

    /// Reduce a 114-byte SHAKE256 output interpreted little-endian.
    pub(super) fn reduce_wide(bytes: &[u8; 114]) -> Self {
        Self::reduce_bytes(bytes)
    }

    /// Reduce a 57-byte secret scalar interpreted little-endian.
    pub(super) fn reduce_57(bytes: &[u8; 57]) -> Self {
        Self::reduce_bytes(bytes)
    }

    /// Decode `S`, rejecting values `>= L` as §5.2.7 requires.
    pub(super) fn from_canonical_bytes(bytes: &[u8; 57]) -> Option<Self> {
        let mut padded = [0_u8; 64];
        padded[..57].copy_from_slice(bytes);
        let limbs = core::array::from_fn(|i| {
            u64::from_le_bytes(padded[i * 8..i * 8 + 8].try_into().unwrap())
        });
        let (_, borrow) = subtract_limbs(limbs, order_limbs());
        if borrow == 1 {
            Some(Self { limbs })
        } else {
            None
        }
    }

    pub(super) fn add(&self, right: &Self) -> Self {
        let mut sum = [0_u64; LIMBS];
        let mut carry = 0_u64;
        for (index, output) in sum.iter_mut().enumerate() {
            let (first, c1) = self.limbs[index].overflowing_add(right.limbs[index]);
            let (second, c2) = first.overflowing_add(carry);
            *output = second;
            carry = u64::from(c1 | c2);
        }
        debug_assert_eq!(carry, 0, "two residues below L fit in 447 bits");
        Self {
            limbs: subtract_order_if_needed(sum),
        }
    }

    /// Multiply two residues with a fixed 456-step double-and-add schedule.
    pub(super) fn multiply(&self, right: &Self) -> Self {
        let mut result = Self::ZERO;
        let mut addend = Self { limbs: self.limbs };
        for bit_index in 0..456 {
            let candidate = result.add(&addend);
            let bit = (right.limbs[bit_index / 64] >> (bit_index % 64)) & 1;
            result = Self::conditional_select(&result, &candidate, bit);
            addend = addend.add(&addend);
        }
        result
    }

    /// The canonical little-endian 57-byte encoding.
    pub(super) fn to_bytes(&self) -> [u8; 57] {
        let mut output = [0_u8; 64];
        for (index, limb) in self.limbs.iter().enumerate() {
            output[index * 8..index * 8 + 8].copy_from_slice(&limb.to_le_bytes());
        }
        output[..57].try_into().expect("57 bytes")
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
        let mut doubled = [0_u64; LIMBS];
        let mut carry = bit;
        for (index, output) in doubled.iter_mut().enumerate() {
            let next_carry = self.limbs[index] >> 63;
            *output = (self.limbs[index] << 1) | carry;
            carry = next_carry;
        }
        debug_assert_eq!(carry, 0, "twice a residue below L fits in 447 bits");
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

fn subtract_limbs(left: [u64; LIMBS], right: [u64; LIMBS]) -> ([u64; LIMBS], u64) {
    let mut difference = [0_u64; LIMBS];
    let mut borrow = 0_u64;
    for index in 0..LIMBS {
        let (first, b1) = left[index].overflowing_sub(right[index]);
        let (second, b2) = first.overflowing_sub(borrow);
        difference[index] = second;
        borrow = u64::from(b1 | b2);
    }
    (difference, borrow)
}

fn subtract_order_if_needed(value: [u64; LIMBS]) -> [u64; LIMBS] {
    let (mut difference, borrow) = subtract_limbs(value, order_limbs());
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
        let mut below = ORDER_BYTES;
        below[0] -= 1;
        assert!(Scalar::from_canonical_bytes(&below).is_some());
        assert!(Scalar::from_canonical_bytes(&ORDER_BYTES).is_none());
    }

    #[test]
    fn reduction_maps_order_to_zero_and_multiplication_matches_small_integers() {
        assert_eq!(Scalar::reduce_57(&ORDER_BYTES).to_bytes(), [0_u8; 57]);
        let mut a = [0_u8; 57];
        a[0] = 19;
        let mut b = [0_u8; 57];
        b[0] = 23;
        let product = Scalar::reduce_57(&a)
            .multiply(&Scalar::reduce_57(&b))
            .to_bytes();
        assert_eq!(u16::from_le_bytes([product[0], product[1]]), 437);
        assert_eq!(
            Scalar::reduce_57(&a).add(&Scalar::reduce_57(&b)).to_bytes()[0],
            42
        );
    }
}
