//! The exact `GF(2^128)` block multiplication used by GHASH.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §6.3, Algorithm 1][sp-800-38d] defines the product `X • Y`. Step 1 reads
//! the bits of `X` in displayed string order as `x_0` through `x_127`. Step 2 initializes a zero
//! accumulator `Z_0` and a changing multiple `V_0 = Y`. Step 3 conditionally adds `V_i` to the
//! accumulator, shifts `V_i` right, and conditionally adds the reduction block
//! `R = 11100001 || 0^120`. Step 4 returns `Z_128`.
//!
//! The standard calls its polynomial convention “little endian”: the *leftmost* displayed bit
//! `x_0` is the coefficient of `u^0`, even though ordinary integer notation treats that bit as
//! most significant. The implementation therefore keeps the wire block as sixteen ordered bytes
//! and names every conversion. It never reinterprets the bytes as a native-endian integer.
//!
//! This module does not own §6.4's GHASH recurrence, input padding, GCM length encoding, or tag
//! construction. Those operations will compose this exact field product in later layers.
//!
//! ## Calculation and secret policy
//!
//! The reference path performs exactly 128 iterations and uses word masks rather than tables or
//! source-level branches selected by operand bits. This structure has not received compiler-output
//! or platform-level constant-time analysis and is not a side-channel-resistance claim. SP
//! 800-38D §5.3 requires the GCM hash subkey and intermediate values to remain secret, so field
//! elements and temporary blocks are non-`Clone` and zeroize on drop or before return.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

use zeroize::Zeroize;

/// Number of bytes in every SP 800-38D block.
const BLOCK_BYTES: usize = 16;

/// First byte of §6.3's reduction block `R = 11100001 || 0^120`.
///
/// The remaining fifteen bytes are zero, so the reduction addition only changes byte zero.
const REDUCTION_FIRST_BYTE: u8 = 0xe1;

/// One private field element in the exact byte order used on the GCM boundary.
///
/// The block remains byte-oriented to preserve §6.3's unusual mapping between displayed bits and
/// polynomial coefficients. It is non-`Clone` and non-`Copy`; dropping it clears its byte array.
pub(super) struct FieldElement {
    bytes: [u8; BLOCK_BYTES],
}

impl FieldElement {
    /// Copy one complete GCM block into the field representation without reordering any bit.
    #[cfg(test)]
    #[must_use]
    pub(super) fn from_block(block: &[u8; BLOCK_BYTES]) -> Self {
        Self { bytes: *block }
    }

    /// Move one owned GCM block into the field representation without leaving a second copy.
    ///
    /// Secret subkey construction uses this boundary so the caller's array becomes the
    /// zeroizing [`FieldElement`] storage rather than being copied from a non-zeroizing local.
    #[must_use]
    pub(super) fn from_owned_block(block: [u8; BLOCK_BYTES]) -> Self {
        Self { bytes: block }
    }

    /// Construct Algorithm 2 step 2's `Y_0 = 0^128` without an encoding conversion.
    #[must_use]
    pub(super) fn zero() -> Self {
        Self {
            bytes: [0_u8; BLOCK_BYTES],
        }
    }

    /// Add one complete input block coefficient-by-coefficient using bitwise XOR.
    ///
    /// SP 800-38D §6.3 defines XOR as field addition, and §6.4 Algorithm 2 step 3 requires
    /// `Y_(i-1) XOR X_i` before every multiplication by `H`.
    pub(super) fn add_block(&mut self, block: &[u8; BLOCK_BYTES]) {
        for (accumulator_byte, input_byte) in self.bytes.iter_mut().zip(block) {
            *accumulator_byte ^= input_byte;
        }
    }

    /// Borrow the complete field result in unchanged GCM block order.
    #[must_use]
    pub(super) fn as_block(&self) -> &[u8; BLOCK_BYTES] {
        &self.bytes
    }

    /// Transfer the complete field value to the next zeroizing owner.
    #[must_use]
    pub(super) fn into_block(mut self) -> [u8; BLOCK_BYTES] {
        core::mem::take(&mut self.bytes)
    }
}

impl Drop for FieldElement {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Read Algorithm 1's coefficients from the most significant bit of each wire byte.
const fn displayed_bit(byte: u8, index: u8) -> u8 {
    (byte >> (7 - index)) & 1
}

/// Apply Algorithm 1 step 3's update from `V_i` to `V_(i+1)`.
///
/// The most significant byte of the displayed string contains the reduction coefficient.
const fn advance_changing_multiple(block: u128) -> u128 {
    let reduction = (REDUCTION_FIRST_BYTE as u128) << 120;
    (block >> 1) ^ (reduction & 0_u128.wrapping_sub(block & 1))
}

/// SP 800-38D §6.3, Algorithm 1. Visit the left operand in wire order while
/// one integer shift advances the entire changing multiple.
#[must_use]
pub(super) fn multiply(left: &FieldElement, right: &FieldElement) -> FieldElement {
    let mut changing_multiple = u128::from_be_bytes(right.bytes);
    let mut accumulator = 0_u128;
    for &byte in &left.bytes {
        for bit in 0..8 {
            let mask = 0_u128.wrapping_sub(u128::from(displayed_bit(byte, bit)));
            accumulator ^= changing_multiple & mask;
            changing_multiple = advance_changing_multiple(changing_multiple);
        }
    }
    let result = FieldElement::from_owned_block(accumulator.to_be_bytes());
    accumulator.zeroize();
    changing_multiple.zeroize();
    result
}

#[cfg(test)]
mod unit {
    use super::{BLOCK_BYTES, FieldElement, advance_changing_multiple, displayed_bit, multiply};

    /// Differential against Algorithm 1 for every product of two monomials.
    #[cfg(feature = "aes-gcm-accel")]
    #[test]
    fn accelerated_products_match_all_bit_positions() {
        let Some(cpu) = rsl_aes_gcm_accel::AesGcmCpu::detect() else {
            return;
        };
        for i in 0..128 {
            for j in 0..128 {
                let left = FieldElement::from_owned_block((1_u128 << i).to_be_bytes());
                let right = FieldElement::from_owned_block((1_u128 << j).to_be_bytes());
                assert_eq!(
                    cpu.multiply(left.as_block(), right.as_block()),
                    *multiply(&left, &right).as_block(),
                    "bits {i} and {j}"
                );
            }
        }
    }

    /// Standard-derived representation evidence from SP 800-38D §6.3, Algorithm 1 step 1.
    #[test]
    fn displayed_bits_are_read_left_to_right_across_bytes() {
        let bytes = [0b1000_0001, 0b0100_0000];
        let bits = bytes
            .map(|byte| core::array::from_fn(|i| displayed_bit(byte, u8::try_from(i).unwrap())));
        assert_eq!(bits, [[1, 0, 0, 0, 0, 0, 0, 1], [0, 1, 0, 0, 0, 0, 0, 0]]);
    }

    /// Standard-derived transition evidence from SP 800-38D §6.3, Algorithm 1 step 3.
    #[test]
    fn changing_multiple_shifts_without_reduction_when_its_last_bit_is_zero() {
        let mut block = [0_u8; BLOCK_BYTES];
        block[0] = 0x80;

        let block = advance_changing_multiple(u128::from_be_bytes(block)).to_be_bytes();

        let mut expected = [0_u8; BLOCK_BYTES];
        expected[0] = 0x40;
        assert_eq!(block, expected);
    }

    /// Standard-derived reduction evidence from SP 800-38D §6.3's definition of `R` and
    /// Algorithm 1 step 3.
    #[test]
    fn changing_multiple_adds_r_when_its_discarded_bit_is_one() {
        let mut block = [0_u8; BLOCK_BYTES];
        block[BLOCK_BYTES - 1] = 1;

        let block = advance_changing_multiple(u128::from_be_bytes(block)).to_be_bytes();

        let mut expected_r = [0_u8; BLOCK_BYTES];
        expected_r[0] = 0xe1;
        assert_eq!(block, expected_r);
    }

    /// Standard-derived field-identity evidence from SP 800-38D §6.3's polynomial convention.
    ///
    /// Because the leftmost displayed bit is the coefficient of `u^0`, the multiplicative
    /// identity is `0x80 || 0^120`, not the integer-looking block ending in `0x01`.
    #[test]
    fn the_leftmost_bit_is_the_multiplicative_identity() {
        let mut identity_bytes = [0_u8; BLOCK_BYTES];
        identity_bytes[0] = 0x80;
        let identity = FieldElement::from_block(&identity_bytes);
        let operand = FieldElement::from_block(&[
            0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5,
            0x3b, 0x78,
        ]);

        assert_eq!(multiply(&identity, &operand).as_block(), operand.as_block());
        assert_eq!(multiply(&operand, &identity).as_block(), operand.as_block());
    }

    /// Standard-derived zero behavior from Algorithm 1's zero accumulator and masked additions.
    #[test]
    fn zero_annihilates_either_operand() {
        let zero = FieldElement::from_block(&[0_u8; BLOCK_BYTES]);
        let operand = FieldElement::from_block(&[
            0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
            0xd4, 0x9c,
        ]);

        assert_eq!(multiply(&zero, &operand).as_block(), &[0_u8; BLOCK_BYTES]);
        assert_eq!(multiply(&operand, &zero).as_block(), &[0_u8; BLOCK_BYTES]);
    }

    /// Standard-derived multiplication evidence using published operands from NIST's
    /// `AES_GCM.pdf`, GCM-AES128 Example 2.
    ///
    /// NIST publishes the first ciphertext block and `H`; it does not print their individual
    /// product. The expected product is therefore explicitly classified as derived by applying
    /// SP 800-38D §6.3, Algorithm 1, not as a published known answer.
    #[test]
    fn first_example_two_ciphertext_block_times_h_matches_the_derived_product() {
        let ciphertext_block = FieldElement::from_block(&[
            0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
            0xd4, 0x9c,
        ]);
        let hash_subkey = FieldElement::from_block(&[
            0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5,
            0x3b, 0x78,
        ]);
        let expected = [
            0x59, 0xed, 0x3f, 0x2b, 0xb1, 0xa0, 0xaa, 0xa0, 0x7c, 0x9f, 0x56, 0xc6, 0xa5, 0x04,
            0x64, 0x7b,
        ];

        assert_eq!(
            multiply(&ciphertext_block, &hash_subkey).as_block(),
            &expected
        );
        assert_eq!(
            multiply(&hash_subkey, &ciphertext_block).as_block(),
            &expected
        );
    }
}
