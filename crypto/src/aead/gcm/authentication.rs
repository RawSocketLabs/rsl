//! Construction of GCM's authenticated GHASH input.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §7.1 Algorithm 4 steps 4–5][sp-800-38d] and §7.2 Algorithm 5 steps 5–6
//! define the same secret block `S`:
//!
//! `GHASH_H(A || 0^v || C || 0^u || [len(A)]_64 || [len(C)]_64)`.
//!
//! `A` is additional authenticated data and `C` is ciphertext. `v` and `u` are the minimum zero
//! padding needed to end each byte-oriented input at a 128-bit boundary. The final block contains
//! the two original bit lengths as independent 64-bit big-endian integers. This module feeds those
//! blocks into the already-tested §6.4 recurrence without allocating the conceptual concatenation.
//!
//! This layer does not derive `H`, encrypt plaintext, mask `S` into a tag, compare received tags,
//! or enforce the smaller GCM plaintext/ciphertext limit from §5.2.1. The later complete
//! construction validates all public input limits before invoking any transformation. This layer
//! checks that both byte lengths can be represented as the required 64-bit bit counts.
//!
//! ## Evidence and lifetime
//!
//! Complete input blocks are borrowed. A final partial block and the length block use short local
//! arrays that are zeroized after the recurrence copies their contribution into its secret
//! accumulator. [`GhashResult`] keeps `S` in a non-`Clone`, zeroizing owner until tag masking.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private GCM authentication-input construction lands before tag composition"
    )
)]

use zeroize::Zeroize;

use super::ghash::{Ghash, GhashResult, HashSubkey};
use crate::{CryptoError, Result};

/// Number of bytes in one GHASH input block.
const BLOCK_BYTES: usize = 16;

/// Convert a byte length to SP 800-38D's required 64-bit bit-length representation.
///
/// Conversion and multiplication are checked separately so this remains correct on platforms
/// where `usize` is wider than `u64` as well as when the factor of eight itself overflows.
fn bit_length(byte_length: usize) -> Result<u64> {
    u64::try_from(byte_length)
        .ok()
        .and_then(|length| length.checked_mul(u8::BITS.into()))
        .ok_or(CryptoError::MessageTooLong)
}

/// Encode `[len(A)]_64 || [len(C)]_64` from Algorithm 4 step 5.
fn length_block(associated_data_len: usize, ciphertext_len: usize) -> Result<[u8; BLOCK_BYTES]> {
    let associated_data_bits = bit_length(associated_data_len)?;
    let ciphertext_bits = bit_length(ciphertext_len)?;
    let mut block = [0_u8; BLOCK_BYTES];

    block[..8].copy_from_slice(&associated_data_bits.to_be_bytes());
    block[8..].copy_from_slice(&ciphertext_bits.to_be_bytes());

    Ok(block)
}

/// Feed one byte string followed by its minimum zero padding into the GHASH recurrence.
fn update_zero_padded(ghash: &mut Ghash, input: &[u8]) {
    let mut complete_blocks = input.chunks_exact(BLOCK_BYTES);

    for block in &mut complete_blocks {
        let exact_block = <&[u8; BLOCK_BYTES]>::try_from(block)
            .expect("chunks_exact always yields one complete GHASH block");
        ghash.update_block(exact_block);
    }

    let remainder = complete_blocks.remainder();
    if !remainder.is_empty() {
        let mut padded = [0_u8; BLOCK_BYTES];
        padded[..remainder.len()].copy_from_slice(remainder);
        ghash.update_block(&padded);
        padded.zeroize();
    }
}

/// Calculate the GCM authentication intermediate `S` for AAD and ciphertext byte strings.
///
/// # Errors
///
/// Returns [`CryptoError::MessageTooLong`] before starting GHASH if either byte length cannot be
/// represented as a 64-bit bit count.
pub(super) fn calculate_s(
    hash_subkey: HashSubkey,
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Result<GhashResult> {
    let mut final_length_block = length_block(associated_data.len(), ciphertext.len())?;
    let mut ghash = Ghash::new(hash_subkey);

    update_zero_padded(&mut ghash, associated_data);
    update_zero_padded(&mut ghash, ciphertext);
    ghash.update_block(&final_length_block);
    final_length_block.zeroize();

    Ok(ghash.finalize())
}

#[cfg(test)]
mod unit {
    use super::{CryptoError, HashSubkey, bit_length, calculate_s, length_block};

    const HASH_SUBKEY: [u8; 16] = [
        0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5, 0x3b,
        0x78,
    ];

    const ASSOCIATED_DATA: [u8; 64] = [
        0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66, 0xef,
        0x97, 0xf5, 0xd3, 0xd5, 0x85, 0x03, 0xb9, 0x69, 0x9d, 0xe7, 0x85, 0x89, 0x5a, 0x96, 0xfd,
        0xba, 0xaf, 0x43, 0xb1, 0xcd, 0x7f, 0x59, 0x8e, 0xce, 0x23, 0x88, 0x1b, 0x00, 0xe3, 0xed,
        0x03, 0x06, 0x88, 0x7b, 0x0c, 0x78, 0x5e, 0x27, 0xe8, 0xad, 0x3f, 0x82, 0x23, 0x20, 0x71,
        0x04, 0x72, 0x5d, 0xd4,
    ];

    const CIPHERTEXT: [u8; 64] = [
        0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0, 0xd4,
        0x9c, 0xe3, 0xaa, 0x21, 0x2f, 0x2c, 0x02, 0xa4, 0xe0, 0x35, 0xc1, 0x7e, 0x23, 0x29, 0xac,
        0xa1, 0x2e, 0x21, 0xd5, 0x14, 0xb2, 0x54, 0x66, 0x93, 0x1c, 0x7d, 0x8f, 0x6a, 0x5a, 0xac,
        0x84, 0xaa, 0x05, 0x1b, 0xa3, 0x0b, 0x39, 0x6a, 0x0a, 0xac, 0x97, 0x3d, 0x58, 0xe0, 0x91,
        0x47, 0x3f, 0x59, 0x85,
    ];

    /// Standard-derived encoding evidence from Algorithm 4 steps 4–5.
    #[test]
    fn byte_lengths_become_two_independent_big_endian_bit_lengths() {
        assert_eq!(
            length_block(20, 60),
            Ok([
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x01, 0xe0,
            ])
        );
    }

    /// Standard-derived bound evidence for `[len(X)]_64`.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn bit_length_rejects_the_first_unrepresentable_byte_length() {
        let largest_byte_length = usize::try_from(u64::MAX / 8)
            .expect("the test target can represent the largest whole-byte GCM bit length");

        assert_eq!(bit_length(largest_byte_length), Ok((u64::MAX / 8) * 8));
        assert_eq!(
            bit_length(largest_byte_length + 1),
            Err(CryptoError::MessageTooLong)
        );
    }

    /// Published empty-AAD evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2, `S`.
    #[test]
    fn example_two_authenticates_complete_ciphertext_blocks() {
        let result = calculate_s(HashSubkey::new(HASH_SUBKEY), &[], &CIPHERTEXT)
            .expect("the published lengths fit in 64-bit bit counts");

        assert_eq!(
            result.as_block(),
            &[
                0x7f, 0x1b, 0x32, 0xb8, 0x1b, 0x82, 0x0d, 0x02, 0x61, 0x4f, 0x88, 0x95, 0xac, 0x1d,
                0x4e, 0xac,
            ]
        );
    }

    /// Published empty-ciphertext evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 3, `S`.
    #[test]
    fn example_three_authenticates_complete_aad_blocks() {
        let result = calculate_s(HashSubkey::new(HASH_SUBKEY), &ASSOCIATED_DATA, &[])
            .expect("the published lengths fit in 64-bit bit counts");

        assert_eq!(
            result.as_block(),
            &[
                0x6d, 0xd6, 0xcf, 0x3a, 0x1f, 0xa0, 0x37, 0x1d, 0xd4, 0xc5, 0xc1, 0xac, 0x1c, 0x36,
                0x75, 0xf1,
            ]
        );
    }

    /// Published AAD-and-ciphertext evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 4, `S`.
    #[test]
    fn example_four_preserves_aad_then_ciphertext_order() {
        let result = calculate_s(HashSubkey::new(HASH_SUBKEY), &ASSOCIATED_DATA, &CIPHERTEXT)
            .expect("the published lengths fit in 64-bit bit counts");

        assert_eq!(
            result.as_block(),
            &[
                0x56, 0x87, 0x3b, 0x62, 0x38, 0xe0, 0x50, 0x2e, 0x16, 0xdb, 0x13, 0x23, 0xd4, 0x1e,
                0xb6, 0x55,
            ]
        );
    }

    /// Published dual-partial-padding evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 5.
    #[test]
    fn example_five_zero_pads_aad_and_ciphertext_independently() {
        let result = calculate_s(
            HashSubkey::new(HASH_SUBKEY),
            &ASSOCIATED_DATA[..20],
            &CIPHERTEXT[..60],
        )
        .expect("the published lengths fit in 64-bit bit counts");

        assert_eq!(
            result.as_block(),
            &[
                0xc2, 0x3b, 0x3d, 0x63, 0xd2, 0xed, 0x95, 0x05, 0x6c, 0xa3, 0x42, 0x76, 0x9c, 0xd1,
                0x3c, 0x03,
            ]
        );
    }
}
