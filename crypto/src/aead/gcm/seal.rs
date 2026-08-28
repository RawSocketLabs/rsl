//! Private composition of GCM authenticated encryption.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §7.1, Algorithm 4][sp-800-38d] defines authenticated encryption. This
//! module keeps its six steps visible in [`seal`]:
//!
//! 1. derive `H = CIPH_K(0^128)`;
//! 2. construct `J_0 = IV || 0^31 || 1` for the supported 96-bit IV;
//! 3. calculate `C = GCTR_K(inc32(J_0), P)`;
//! 4. zero-pad AAD and ciphertext independently;
//! 5. calculate `S` with GHASH over those values and their encoded bit lengths; and
//! 6. calculate the full tag `T = GCTR_K(J_0, S)`.
//!
//! The length check immediately before step 1 implements §5.2.1.1. It happens before plaintext
//! is copied or transformed, so an invalid invocation cannot return a partial result. The caller
//! remains responsible for the critical §8 requirement that a key/IV pair is never reused.
//!
//! This is deliberately still a private composition layer. Its result keeps ciphertext and tag
//! semantically distinct, cannot be cloned, and transfers ownership explicitly. A later public
//! AES-128-GCM type will provide the stable nonce, tag, and detached-output API only after the
//! matching verify-before-decrypt path has been validated.
//!
//! ## Evidence
//!
//! Composition tests use the published intermediate and final values in NIST's
//! [`AES_GCM.pdf`][examples]. Example 1 exercises the empty-input rule, Example 2 exercises four
//! complete plaintext blocks, and Example 5 exercises independent partial AAD and ciphertext
//! padding plus a partial final GCTR block.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf
//! [examples]: https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_GCM.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private authenticated encryption lands before the public AES-128-GCM boundary"
    )
)]

use super::{
    authentication::calculate_s,
    gctr,
    limits::validate_input_lengths,
    setup::{GcmIv96, PreCounterBlock, derive_hash_subkey},
    tag::{self, FullTag},
};
use crate::{Result, cipher::aes::aes128::Aes128};
use alloc::vec::Vec;

/// Ciphertext and a detached full tag produced by the private GCM composition.
pub(super) struct EncryptionResult {
    ciphertext: Vec<u8>,
    tag: FullTag,
}

impl EncryptionResult {
    /// Borrow the complete ciphertext.
    #[must_use]
    pub(super) fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Borrow the complete detached tag.
    #[must_use]
    pub(super) fn tag(&self) -> &FullTag {
        &self.tag
    }

    /// Transfer both outputs to the next owners.
    ///
    /// The caller becomes responsible for clearing the returned ciphertext allocation and tag
    /// array if its context requires that lifetime policy.
    #[must_use]
    pub(super) fn into_parts(self) -> (Vec<u8>, [u8; 16]) {
        (self.ciphertext, self.tag.into_bytes())
    }
}

/// Encrypt and authenticate one byte-oriented input under the private 96-bit-IV profile.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] before transformation if the AAD or plaintext
/// exceeds the limits defined by SP 800-38D §5.2.1.1.
pub(super) fn seal(
    cipher: &Aes128,
    iv: &GcmIv96,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<EncryptionResult> {
    validate_input_lengths(associated_data.len(), plaintext.len())?;

    // Algorithm 4, step 1.
    let hash_subkey = derive_hash_subkey(cipher);

    // Algorithm 4, step 2. The type fixes the IV at the recommended 96-bit length.
    let pre_counter = PreCounterBlock::from_iv(iv);

    // Algorithm 4, step 3. This allocation is the only owned output buffer; it initially holds a
    // copy of P and is transformed completely in place into C.
    let mut ciphertext = plaintext.to_vec();
    gctr::apply(cipher, pre_counter.payload_counter(), &mut ciphertext);

    // Algorithm 4, steps 4–5. The authentication input contains C, never P.
    let ghash_result = calculate_s(hash_subkey, associated_data, &ciphertext)?;

    // Algorithm 4, step 6. This profile keeps all 128 tag bits.
    let tag = tag::mask(cipher, pre_counter, ghash_result);

    Ok(EncryptionResult { ciphertext, tag })
}

#[cfg(test)]
mod unit {
    use super::{Aes128, GcmIv96, seal};
    use crate::cipher::aes::aes128::Aes128Key;

    const PLAINTEXT: [u8; 64] = [
        0xd9, 0x31, 0x32, 0x25, 0xf8, 0x84, 0x06, 0xe5, 0xa5, 0x59, 0x09, 0xc5, 0xaf, 0xf5, 0x26,
        0x9a, 0x86, 0xa7, 0xa9, 0x53, 0x15, 0x34, 0xf7, 0xda, 0x2e, 0x4c, 0x30, 0x3d, 0x8a, 0x31,
        0x8a, 0x72, 0x1c, 0x3c, 0x0c, 0x95, 0x95, 0x68, 0x09, 0x53, 0x2f, 0xcf, 0x0e, 0x24, 0x49,
        0xa6, 0xb5, 0x25, 0xb1, 0x6a, 0xed, 0xf5, 0xaa, 0x0d, 0xe6, 0x57, 0xba, 0x63, 0x7b, 0x39,
        0x1a, 0xaf, 0xd2, 0x55,
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

    /// Construct the common key and IV from NIST GCM-AES128 Examples 1–5.
    fn nist_cipher_and_iv() -> (Aes128, GcmIv96) {
        (
            Aes128::new(Aes128Key::new([
                0xfe, 0xff, 0xe9, 0x92, 0x86, 0x65, 0x73, 0x1c, 0x6d, 0x6a, 0x8f, 0x94, 0x67, 0x30,
                0x83, 0x08,
            ])),
            GcmIv96::new([
                0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
            ]),
        )
    }

    /// Published final-output evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 1.
    #[test]
    fn empty_plaintext_and_aad_produce_the_published_tag() {
        let (cipher, iv) = nist_cipher_and_iv();
        let result = seal(&cipher, &iv, &[], &[]).expect("empty inputs satisfy every length limit");

        assert!(result.ciphertext().is_empty());
        assert_eq!(
            result.tag().as_bytes(),
            &[
                0x32, 0x47, 0x18, 0x4b, 0x3c, 0x4f, 0x69, 0xa4, 0x4d, 0xbc, 0xd2, 0x28, 0x87, 0xbb,
                0xb4, 0x18,
            ]
        );
    }

    /// Published complete-block composition evidence from NIST GCM-AES128 Example 2.
    #[test]
    fn complete_plaintext_blocks_produce_the_published_ciphertext_and_tag() {
        let (cipher, iv) = nist_cipher_and_iv();
        let result = seal(&cipher, &iv, &[], &PLAINTEXT)
            .expect("the published inputs satisfy every length limit");

        assert_eq!(result.ciphertext(), CIPHERTEXT);
        assert_eq!(
            result.tag().as_bytes(),
            &[
                0x4d, 0x5c, 0x2a, 0xf3, 0x27, 0xcd, 0x64, 0xa6, 0x2c, 0xf3, 0x5a, 0xbd, 0x2b, 0xa6,
                0xfa, 0xb4,
            ]
        );
    }

    /// Published partial-block composition evidence from NIST GCM-AES128 Example 5.
    #[test]
    fn partial_aad_and_plaintext_produce_the_published_ciphertext_and_tag() {
        let (cipher, iv) = nist_cipher_and_iv();
        let result = seal(&cipher, &iv, &ASSOCIATED_DATA[..20], &PLAINTEXT[..60])
            .expect("the published inputs satisfy every length limit");

        assert_eq!(result.ciphertext(), &CIPHERTEXT[..60]);
        assert_eq!(
            result.tag().as_bytes(),
            &[
                0xf0, 0x7c, 0x25, 0x28, 0xee, 0xa2, 0xfc, 0xa1, 0x21, 0x1f, 0x90, 0x5e, 0x1b, 0x6a,
                0x88, 0x1b,
            ]
        );
    }

    /// Ownership evidence for the later public detached-result boundary.
    #[test]
    fn result_transfers_ciphertext_and_tag_without_a_second_owner() {
        let (cipher, iv) = nist_cipher_and_iv();
        let result = seal(&cipher, &iv, &[], &[]).expect("empty inputs are valid");
        let (ciphertext, tag) = result.into_parts();

        assert!(ciphertext.is_empty());
        assert_eq!(tag.len(), 16);
    }
}
