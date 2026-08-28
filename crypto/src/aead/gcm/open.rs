//! Private composition of GCM authenticated decryption.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §7.2, Algorithm 5][sp-800-38d] defines authenticated decryption as a
//! plaintext or `FAIL` operation. Its printed step order calculates plaintext before the candidate
//! tag, but the paragraph following step 8 explicitly permits tag verification to precede
//! plaintext computation. [`open`] uses that permitted ordering:
//!
//! 1. validate the supported AAD and ciphertext lengths;
//! 2. derive `H = CIPH_K(0^128)` and construct the 96-bit-IV `J_0`;
//! 3. calculate `S` from AAD and the received ciphertext;
//! 4. calculate and compare the candidate full tag;
//! 5. only after successful comparison, allocate a buffer and calculate
//!    `P = GCTR_K(inc32(J_0), C)`.
//!
//! Thus no plaintext owner exists on an authentication-failure path. AAD, ciphertext, IV, and
//! tag changes all reach the same [`crate::CryptoError::AuthenticationFailed`] boundary. As with
//! encryption, replay detection and key/IV lifecycle policy belong to the consuming protocol.
//!
//! This composition remains private until the stable AES-128-GCM nonce/tag types and public
//! authenticated-encryption contract are added.
//!
//! ## Evidence
//!
//! The positive tests recover published plaintext from NIST's [`AES_GCM.pdf`][examples]. Negative
//! tests independently alter AAD, ciphertext, IV, and tag, and observe only authentication failure.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf
//! [examples]: https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_GCM.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private authenticated decryption lands before the public AES-128-GCM boundary"
    )
)]

use alloc::vec::Vec;

use super::{
    authentication::calculate_s,
    gctr,
    limits::validate_input_lengths,
    setup::{GcmIv96, PreCounterBlock, derive_hash_subkey},
    tag,
};
use crate::{Result, cipher::aes::aes128::Aes128};

/// Authenticate and decrypt one byte-oriented input under the private 96-bit-IV profile.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] before authentication if the AAD or ciphertext
/// exceeds the limits defined by SP 800-38D §5.2.1.1. Returns
/// [`crate::CryptoError::AuthenticationFailed`] without allocating or transforming plaintext when
/// the received tag does not authenticate the exact IV, AAD, and ciphertext.
pub(super) fn open(
    cipher: &Aes128,
    iv: &GcmIv96,
    associated_data: &[u8],
    ciphertext: &[u8],
    received_tag: &[u8; 16],
) -> Result<Vec<u8>> {
    // Algorithm 5, step 1. The distinct IV and tag types already enforce their exact lengths.
    validate_input_lengths(associated_data.len(), ciphertext.len())?;

    // Algorithm 5, steps 2–3.
    let hash_subkey = derive_hash_subkey(cipher);
    let pre_counter = PreCounterBlock::from_iv(iv);

    // Algorithm 5, steps 5–7, intentionally evaluated before step 4 as permitted after step 8.
    let ghash_result = calculate_s(hash_subkey, associated_data, ciphertext)?;
    let candidate_tag = tag::mask(cipher, pre_counter, ghash_result);
    candidate_tag.verify(received_tag)?;

    // Algorithm 5, step 4. This plaintext allocation is unreachable for an invalid tag.
    let mut plaintext = ciphertext.to_vec();
    let pre_counter = PreCounterBlock::from_iv(iv);
    gctr::apply(cipher, pre_counter.payload_counter(), &mut plaintext);

    Ok(plaintext)
}

#[cfg(test)]
mod unit {
    use super::{Aes128, GcmIv96, open};
    use crate::{CryptoError, cipher::aes::aes128::Aes128Key};

    const PLAINTEXT: [u8; 64] = [
        0xd9, 0x31, 0x32, 0x25, 0xf8, 0x84, 0x06, 0xe5, 0xa5, 0x59, 0x09, 0xc5, 0xaf, 0xf5, 0x26,
        0x9a, 0x86, 0xa7, 0xa9, 0x53, 0x15, 0x34, 0xf7, 0xda, 0x2e, 0x4c, 0x30, 0x3d, 0x8a, 0x31,
        0x8a, 0x72, 0x1c, 0x3c, 0x0c, 0x95, 0x95, 0x68, 0x09, 0x53, 0x2f, 0xcf, 0x0e, 0x24, 0x49,
        0xa6, 0xb5, 0x25, 0xb1, 0x6a, 0xed, 0xf5, 0xaa, 0x0d, 0xe6, 0x57, 0xba, 0x63, 0x7b, 0x39,
        0x1a, 0xaf, 0xd2, 0x55,
    ];

    const ASSOCIATED_DATA: [u8; 20] = [
        0x3a, 0xd7, 0x7b, 0xb4, 0x0d, 0x7a, 0x36, 0x60, 0xa8, 0x9e, 0xca, 0xf3, 0x24, 0x66, 0xef,
        0x97, 0xf5, 0xd3, 0xd5, 0x85,
    ];

    const CIPHERTEXT: [u8; 60] = [
        0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0, 0xd4,
        0x9c, 0xe3, 0xaa, 0x21, 0x2f, 0x2c, 0x02, 0xa4, 0xe0, 0x35, 0xc1, 0x7e, 0x23, 0x29, 0xac,
        0xa1, 0x2e, 0x21, 0xd5, 0x14, 0xb2, 0x54, 0x66, 0x93, 0x1c, 0x7d, 0x8f, 0x6a, 0x5a, 0xac,
        0x84, 0xaa, 0x05, 0x1b, 0xa3, 0x0b, 0x39, 0x6a, 0x0a, 0xac, 0x97, 0x3d, 0x58, 0xe0, 0x91,
    ];

    const TAG: [u8; 16] = [
        0xf0, 0x7c, 0x25, 0x28, 0xee, 0xa2, 0xfc, 0xa1, 0x21, 0x1f, 0x90, 0x5e, 0x1b, 0x6a, 0x88,
        0x1b,
    ];

    /// Construct the key and IV from NIST GCM-AES128 Example 5.
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

    /// Published final-output evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 5.
    #[test]
    fn valid_partial_inputs_recover_only_the_published_plaintext() {
        let (cipher, iv) = nist_cipher_and_iv();
        let plaintext = open(&cipher, &iv, &ASSOCIATED_DATA, &CIPHERTEXT, &TAG)
            .expect("the published tag authenticates the published inputs");

        assert_eq!(plaintext, PLAINTEXT[..60]);
    }

    /// Negative composition evidence for every value bound into the GCM tag.
    #[test]
    fn changed_iv_aad_ciphertext_or_tag_returns_only_authentication_failed() {
        let mut changed_iv = [
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ];
        changed_iv[0] ^= 1;
        let (cipher, _) = nist_cipher_and_iv();
        assert_eq!(
            open(
                &cipher,
                &GcmIv96::new(changed_iv),
                &ASSOCIATED_DATA,
                &CIPHERTEXT,
                &TAG,
            ),
            Err(CryptoError::AuthenticationFailed)
        );

        let mut changed_aad = ASSOCIATED_DATA;
        changed_aad[0] ^= 1;
        let (cipher, iv) = nist_cipher_and_iv();
        assert_eq!(
            open(&cipher, &iv, &changed_aad, &CIPHERTEXT, &TAG),
            Err(CryptoError::AuthenticationFailed)
        );

        let mut changed_ciphertext = CIPHERTEXT;
        changed_ciphertext[0] ^= 1;
        let (cipher, iv) = nist_cipher_and_iv();
        assert_eq!(
            open(&cipher, &iv, &ASSOCIATED_DATA, &changed_ciphertext, &TAG),
            Err(CryptoError::AuthenticationFailed)
        );

        let mut changed_tag = TAG;
        changed_tag[0] ^= 1;
        let (cipher, iv) = nist_cipher_and_iv();
        assert_eq!(
            open(&cipher, &iv, &ASSOCIATED_DATA, &CIPHERTEXT, &changed_tag),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    /// Published empty-input evidence from NIST GCM-AES128 Example 1.
    #[test]
    fn valid_empty_ciphertext_returns_empty_plaintext() {
        let (cipher, iv) = nist_cipher_and_iv();
        let tag = [
            0x32, 0x47, 0x18, 0x4b, 0x3c, 0x4f, 0x69, 0xa4, 0x4d, 0xbc, 0xd2, 0x28, 0x87, 0xbb,
            0xb4, 0x18,
        ];

        let plaintext = open(&cipher, &iv, &[], &[], &tag)
            .expect("the published empty-input tag authenticates");

        assert!(plaintext.is_empty());
    }
}
