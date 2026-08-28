//! Masking the GHASH result into GCM's full authentication tag.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §7.1 Algorithm 4 step 6][sp-800-38d] calculates
//! `T = MSB_t(GCTR_K(J_0, S))`. Section 7.2 Algorithm 5 step 7 calculates the candidate tag by
//! the same rule. This first profile fixes `t = 128`, so `MSB_t` retains the complete one-block
//! GCTR result without truncation.
//!
//! The choice is intentionally narrow. TLS and the initial SSH AES-GCM use cases consume full
//! 128-bit tags, and NIST's pending SP 800-38D revision has not yet published a replacement rule.
//! Supporting another tag size later requires a distinct type and policy review; callers cannot
//! request arbitrary truncation from this layer.
//!
//! This module owns tag masking, the private full-tag representation, and the complete-tag
//! comparison used by authenticated decryption. It does not construct `S`, decide when plaintext
//! may be released, or enforce IV uniqueness.
//!
//! ## Secret lifetime
//!
//! `S` transfers out of [`GhashResult`] into one local block. In-place GCTR transforms that owner
//! into the tag, which is immediately moved into [`FullTag`]. Each owner is non-`Clone` and
//! zeroizes before its storage is discarded. Formatting is deliberately absent.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private full-tag masking lands before authenticated GCM seal/open composition"
    )
)]

use zeroize::Zeroize;

use super::{gctr, ghash::GhashResult, setup::PreCounterBlock};
use crate::{CryptoError, Result, cipher::aes::aes128::Aes128};

/// Number of bytes in the initial GCM profile's full authentication tag.
const TAG_BYTES: usize = 16;

/// One complete 128-bit GCM authentication tag.
pub(super) struct FullTag {
    bytes: [u8; TAG_BYTES],
}

impl FullTag {
    /// Take ownership of one complete tag block.
    #[must_use]
    pub(super) fn from_bytes(bytes: [u8; TAG_BYTES]) -> Self {
        Self { bytes }
    }

    /// Borrow all sixteen tag bytes.
    #[must_use]
    pub(super) fn as_bytes(&self) -> &[u8; TAG_BYTES] {
        &self.bytes
    }

    /// Transfer the tag bytes to the next owner.
    ///
    /// The caller becomes responsible for the returned array's lifetime and destruction.
    #[must_use]
    pub(super) fn into_bytes(mut self) -> [u8; TAG_BYTES] {
        core::mem::take(&mut self.bytes)
    }

    /// Compare this computed tag with one received full tag without value-dependent early exit.
    ///
    /// Every byte participates in the same XOR/OR accumulator before the decision is made. This
    /// source shape is deliberately straightforward to review, but it has not undergone
    /// compiler-output or platform-level constant-time analysis and is not a production-security
    /// claim.
    ///
    /// # Errors
    ///
    /// Returns only [`CryptoError::AuthenticationFailed`] when any received byte differs.
    pub(super) fn verify(self, received: &[u8; TAG_BYTES]) -> Result<()> {
        let mut difference = 0_u8;

        for (computed_byte, received_byte) in self.bytes.iter().zip(received) {
            difference |= computed_byte ^ received_byte;
        }

        if difference == 0 {
            Ok(())
        } else {
            Err(CryptoError::AuthenticationFailed)
        }
    }
}

impl Drop for FullTag {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Calculate the full `GCTR_K(J_0, S)` tag without truncation.
#[must_use]
pub(super) fn mask(
    cipher: &Aes128,
    pre_counter: PreCounterBlock,
    ghash_result: GhashResult,
) -> FullTag {
    let mut bytes = ghash_result.into_block();

    gctr::apply(cipher, pre_counter.into_tag_counter(), &mut bytes);

    FullTag::from_bytes(bytes)
}

#[cfg(test)]
mod unit {
    use super::{Aes128, FullTag, GhashResult, PreCounterBlock, mask};
    use crate::{
        CryptoError,
        aead::gcm::ghash::{Ghash, HashSubkey},
        aead::gcm::setup::GcmIv96,
        cipher::aes::aes128::Aes128Key,
    };

    /// Build a zeroizing `GhashResult` whose bytes equal one published NIST `S` value.
    ///
    /// Updating GHASH under the field identity makes `Y_1 = S`; this test helper avoids exposing
    /// a raw production constructor for the secret result type.
    fn result_from_bytes(bytes: [u8; 16]) -> GhashResult {
        let mut identity = [0_u8; 16];
        identity[0] = 0x80;
        let mut ghash = Ghash::new(HashSubkey::new(identity));
        ghash.update_block(&bytes);
        ghash.finalize()
    }

    /// Construct the common cipher and `J0` used by NIST GCM-AES128 Examples 1–5.
    fn nist_cipher_and_pre_counter() -> (Aes128, PreCounterBlock) {
        let cipher = Aes128::new(Aes128Key::new([
            0xfe, 0xff, 0xe9, 0x92, 0x86, 0x65, 0x73, 0x1c, 0x6d, 0x6a, 0x8f, 0x94, 0x67, 0x30,
            0x83, 0x08,
        ]));
        let iv = GcmIv96::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ]);

        (cipher, PreCounterBlock::from_iv(&iv))
    }

    /// Published full-tag evidence from NIST `AES_GCM.pdf`, GCM-AES128 Examples 1–5.
    #[test]
    fn every_full_length_example_tag_matches_its_published_s_value() {
        let cases = [
            (
                [0_u8; 16],
                [
                    0x32, 0x47, 0x18, 0x4b, 0x3c, 0x4f, 0x69, 0xa4, 0x4d, 0xbc, 0xd2, 0x28, 0x87,
                    0xbb, 0xb4, 0x18,
                ],
            ),
            (
                [
                    0x7f, 0x1b, 0x32, 0xb8, 0x1b, 0x82, 0x0d, 0x02, 0x61, 0x4f, 0x88, 0x95, 0xac,
                    0x1d, 0x4e, 0xac,
                ],
                [
                    0x4d, 0x5c, 0x2a, 0xf3, 0x27, 0xcd, 0x64, 0xa6, 0x2c, 0xf3, 0x5a, 0xbd, 0x2b,
                    0xa6, 0xfa, 0xb4,
                ],
            ),
            (
                [
                    0x6d, 0xd6, 0xcf, 0x3a, 0x1f, 0xa0, 0x37, 0x1d, 0xd4, 0xc5, 0xc1, 0xac, 0x1c,
                    0x36, 0x75, 0xf1,
                ],
                [
                    0x5f, 0x91, 0xd7, 0x71, 0x23, 0xef, 0x5e, 0xb9, 0x99, 0x79, 0x13, 0x84, 0x9b,
                    0x8d, 0xc1, 0xe9,
                ],
            ),
            (
                [
                    0x56, 0x87, 0x3b, 0x62, 0x38, 0xe0, 0x50, 0x2e, 0x16, 0xdb, 0x13, 0x23, 0xd4,
                    0x1e, 0xb6, 0x55,
                ],
                [
                    0x64, 0xc0, 0x23, 0x29, 0x04, 0xaf, 0x39, 0x8a, 0x5b, 0x67, 0xc1, 0x0b, 0x53,
                    0xa5, 0x02, 0x4d,
                ],
            ),
            (
                [
                    0xc2, 0x3b, 0x3d, 0x63, 0xd2, 0xed, 0x95, 0x05, 0x6c, 0xa3, 0x42, 0x76, 0x9c,
                    0xd1, 0x3c, 0x03,
                ],
                [
                    0xf0, 0x7c, 0x25, 0x28, 0xee, 0xa2, 0xfc, 0xa1, 0x21, 0x1f, 0x90, 0x5e, 0x1b,
                    0x6a, 0x88, 0x1b,
                ],
            ),
        ];

        for (published_s, published_tag) in cases {
            let (cipher, pre_counter) = nist_cipher_and_pre_counter();
            let tag = mask(&cipher, pre_counter, result_from_bytes(published_s));
            assert_eq!(tag.as_bytes(), &published_tag);
        }
    }

    /// API-regression evidence: consuming a tag transfers exactly one complete tag block.
    #[test]
    fn full_tag_consumption_is_explicit() {
        let (cipher, pre_counter) = nist_cipher_and_pre_counter();
        let tag: FullTag = mask(&cipher, pre_counter, result_from_bytes([0_u8; 16]));

        assert_eq!(tag.into_bytes().len(), 16);
    }

    /// Negative evidence that every tag position contributes before one uniform decision.
    #[test]
    fn verification_checks_every_byte_and_returns_one_failure_category() {
        let expected = [0xa5; 16];

        assert_eq!(FullTag::from_bytes(expected).verify(&expected), Ok(()));

        for index in 0..expected.len() {
            let mut received = expected;
            received[index] ^= 1;

            assert_eq!(
                FullTag::from_bytes(expected).verify(&received),
                Err(CryptoError::AuthenticationFailed)
            );
        }
    }
}
