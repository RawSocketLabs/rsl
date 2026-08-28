//! GCM hash-subkey derivation and the 96-bit-IV pre-counter block.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D Algorithm 4 step 1 and Algorithm 5 step 2][sp-800-38d] derive the secret GHASH
//! subkey as `H = CIPH_K(0^128)`. Algorithm 4 step 2 and Algorithm 5 step 3 define the pre-counter
//! block `J_0`. When the IV is exactly 96 bits, the required direct construction is
//! `J_0 = IV || 0^31 || 1`, which is the twelve IV bytes followed by `00 00 00 01`.
//!
//! This first profile accepts only a distinct [`GcmIv96`]. The alternative, variable-length-IV
//! branch hashes the IV through GHASH and remains deliberately unimplemented. TLS 1.2/1.3
//! AES-GCM uses a 96-bit nonce, so the fixed-size path reaches the immediate protocol use case
//! without pretending that arbitrary IV lengths are supported. SSH-specific nonce construction
//! remains in its protocol crate and must yield this primitive's exact 96-bit input.
//!
//! ## Type and lifetime boundaries
//!
//! The IV, `J_0`, payload counter, AES block, and GHASH subkey are different types even where
//! their storage lengths coincide. Deriving `H` moves the encrypted zero block directly into the
//! zeroizing [`HashSubkey`] owner. IV and pre-counter values are not secret, but their owned bytes
//! are also cleared on drop so temporary session material has an explicit lifetime.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private SP 800-38D key/IV setup lands before authenticated GCM composition"
    )
)]

use zeroize::Zeroize;

use super::{counter::CounterBlock, ghash::HashSubkey};
use crate::cipher::aes::aes128::{Aes128, Aes128Block};

/// Number of bytes in GCM's directly supported 96-bit IV profile.
const IV_BYTES: usize = 12;

/// Number of bytes in `J_0` and every AES block.
const BLOCK_BYTES: usize = 16;

/// A distinct, owned 96-bit GCM initialization vector.
pub(super) struct GcmIv96 {
    bytes: [u8; IV_BYTES],
}

impl GcmIv96 {
    /// Take ownership of exactly 96 IV bits.
    #[must_use]
    pub(super) fn new(bytes: [u8; IV_BYTES]) -> Self {
        Self { bytes }
    }
}

impl Drop for GcmIv96 {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// The 128-bit GCM pre-counter block `J_0`.
///
/// This remains distinct from [`CounterBlock`]: GCTR for the payload starts at `inc32(J_0)`, while
/// tag masking applies GCTR with `J_0` itself.
pub(super) struct PreCounterBlock {
    bytes: [u8; BLOCK_BYTES],
}

impl PreCounterBlock {
    /// Construct `J_0 = IV || 0^31 || 1` for a 96-bit IV.
    #[must_use]
    pub(super) fn from_iv(iv: &GcmIv96) -> Self {
        let mut bytes = [0_u8; BLOCK_BYTES];
        bytes[..IV_BYTES].copy_from_slice(&iv.bytes);
        bytes[BLOCK_BYTES - 1] = 1;

        Self { bytes }
    }

    /// Copy `J_0`, apply `inc32`, and return GCTR's payload initial counter block.
    #[must_use]
    pub(super) fn payload_counter(&self) -> CounterBlock {
        let mut counter = CounterBlock::new(self.bytes);
        counter.increment();
        counter
    }

    /// Transfer `J_0` into the counter-shaped input used to mask the GHASH result.
    ///
    /// The consumed pre-counter owner is replaced with zeroes before it drops. Unlike
    /// [`Self::payload_counter`], this boundary deliberately does not increment the value.
    #[must_use]
    pub(super) fn into_tag_counter(mut self) -> CounterBlock {
        CounterBlock::new(core::mem::take(&mut self.bytes))
    }

    /// Borrow `J_0` in unchanged block order for white-box setup evidence.
    #[cfg(test)]
    #[must_use]
    fn as_block(&self) -> &[u8; BLOCK_BYTES] {
        &self.bytes
    }
}

impl Drop for PreCounterBlock {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Derive `H = CIPH_K(0^128)` into its distinct secret owner.
#[must_use]
pub(super) fn derive_hash_subkey(cipher: &Aes128) -> HashSubkey {
    let mut encrypted_zero = Aes128Block::new([0_u8; BLOCK_BYTES]);
    cipher.encrypt_block(&mut encrypted_zero);

    HashSubkey::new(encrypted_zero.into_bytes())
}

#[cfg(test)]
mod unit {
    use super::{Aes128, GcmIv96, PreCounterBlock, derive_hash_subkey};
    use crate::cipher::aes::aes128::Aes128Key;

    /// Construct the common AES key from NIST `AES_GCM.pdf` GCM-AES128 Examples 1–6.
    fn nist_example_cipher() -> Aes128 {
        Aes128::new(Aes128Key::new([
            0xfe, 0xff, 0xe9, 0x92, 0x86, 0x65, 0x73, 0x1c, 0x6d, 0x6a, 0x8f, 0x94, 0x67, 0x30,
            0x83, 0x08,
        ]))
    }

    /// Published setup evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2, `H`.
    #[test]
    fn zero_block_encryption_derives_the_published_hash_subkey() {
        let hash_subkey = derive_hash_subkey(&nist_example_cipher());

        assert_eq!(
            hash_subkey.as_block(),
            &[
                0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5,
                0x3b, 0x78,
            ]
        );
    }

    /// Published setup evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2, `IV` and `J0`.
    #[test]
    fn ninety_six_bit_iv_constructs_the_published_pre_counter_block() {
        let iv = GcmIv96::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ]);
        let pre_counter = PreCounterBlock::from_iv(&iv);

        assert_eq!(
            pre_counter.as_block(),
            &[
                0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
                0x00, 0x01,
            ]
        );
    }

    /// Published/standard-derived boundary evidence for Algorithm 4 steps 2–3.
    #[test]
    fn payload_counter_increments_j0_while_tag_counter_retains_it() {
        let first_iv = GcmIv96::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ]);
        let second_iv = GcmIv96::new([
            0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88,
        ]);
        let payload_counter = PreCounterBlock::from_iv(&first_iv).payload_counter();
        let tag_counter = PreCounterBlock::from_iv(&second_iv).into_tag_counter();

        assert_eq!(
            payload_counter.as_block(),
            &[
                0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
                0x00, 0x02,
            ]
        );
        assert_eq!(
            tag_counter.as_block(),
            &[
                0xca, 0xfe, 0xba, 0xbe, 0xfa, 0xce, 0xdb, 0xad, 0xde, 0xca, 0xf8, 0x88, 0x00, 0x00,
                0x00, 0x01,
            ]
        );
    }

    /// Standard-derived position evidence from Algorithm 4 step 2's direct concatenation.
    #[test]
    fn every_iv_byte_retains_its_position_before_the_fixed_suffix() {
        let iv = GcmIv96::new([
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
        ]);
        let pre_counter = PreCounterBlock::from_iv(&iv);

        assert_eq!(
            pre_counter.as_block(),
            &[
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0, 0, 0, 1
            ]
        );
    }
}
