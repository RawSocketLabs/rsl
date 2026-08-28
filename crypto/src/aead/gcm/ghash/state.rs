//! The complete-block GHASH recurrence.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §6.4, Algorithm 2][sp-800-38d] accepts a positive number of complete
//! 128-bit blocks `X_1` through `X_m`, initializes `Y_0 = 0^128`, and calculates
//! `Y_i = (Y_(i-1) XOR X_i) • H` in order. The final `Y_m` is the GHASH result. This module maps
//! one call to [`Ghash::update_block`] to exactly one Algorithm 2 step 3 iteration.
//!
//! This layer accepts complete blocks only. The zero padding and two 64-bit length fields used to
//! form GCM's GHASH input belong to SP 800-38D Algorithms 4 and 5, not Algorithm 2, and will be
//! implemented by the later GCM composition layer. The field product remains isolated in
//! `field.rs` under §6.3.
//!
//! ## Secret lifetime and API boundary
//!
//! SP 800-38D §5.3 requires `H` and intermediate values to remain secret. [`HashSubkey`], the
//! accumulator, and [`GhashResult`] are distinct, non-`Clone`, zeroizing owners. No type in this
//! module is public outside the private GCM implementation, because the standard approves GHASH
//! only inside GCM rather than as a standalone cryptographic hash function.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the private GHASH recurrence lands before SP 800-38D GCM composition consumes it"
    )
)]

use super::field::{FieldElement, multiply};

/// Size of every GHASH input and output block in bytes.
const BLOCK_BYTES: usize = 16;

/// The secret field element `H` that parameterizes one GHASH computation.
///
/// GCM will derive this value as `CIPH_K(0^128)` according to SP 800-38D Algorithms 4 and 5
/// step 1. This constructor currently accepts that already-derived block so Algorithm 2 remains
/// independent from the choice of block cipher.
pub(in crate::aead::gcm) struct HashSubkey(FieldElement);

impl HashSubkey {
    /// Take ownership of one already-derived 128-bit GHASH subkey block.
    #[must_use]
    pub(in crate::aead::gcm) fn new(block: [u8; BLOCK_BYTES]) -> Self {
        Self(FieldElement::from_owned_block(block))
    }

    /// Borrow the derived subkey for white-box GCM setup evidence.
    #[cfg(test)]
    #[must_use]
    pub(in crate::aead::gcm) fn as_block(&self) -> &[u8; BLOCK_BYTES] {
        self.0.as_block()
    }
}

/// Incremental execution of SP 800-38D §6.4, Algorithm 2.
pub(in crate::aead::gcm) struct Ghash {
    hash_subkey: HashSubkey,
    accumulator: FieldElement,
}

impl Ghash {
    /// Consume a hash subkey and initialize `Y_0` to the standard's all-zero block.
    #[must_use]
    pub(in crate::aead::gcm) fn new(hash_subkey: HashSubkey) -> Self {
        Self {
            hash_subkey,
            accumulator: FieldElement::zero(),
        }
    }

    /// Apply one Algorithm 2 step 3 recurrence to one complete input block.
    ///
    /// The exact-size array makes partial input unrepresentable at this layer. Call order is block
    /// order: the first call supplies `X_1`, the second `X_2`, and so on.
    pub(in crate::aead::gcm) fn update_block(&mut self, block: &[u8; BLOCK_BYTES]) {
        self.accumulator.add_block(block);
        self.accumulator = multiply(&self.accumulator, &self.hash_subkey.0);
    }

    /// Consume the recurrence state and retain the final `Y_m` in a zeroizing result owner.
    ///
    /// Algorithm 2 requires a positive block count. This private type does not yet track a count
    /// because GCM's Algorithms 4 and 5 always construct at least their final length block. The
    /// future composition boundary owns and tests that precondition before this becomes reachable.
    #[must_use]
    pub(in crate::aead::gcm) fn finalize(self) -> GhashResult {
        GhashResult(self.accumulator)
    }
}

/// The secret intermediate block returned by GHASH before GCM masks it into a tag.
pub(in crate::aead::gcm) struct GhashResult(FieldElement);

impl GhashResult {
    /// Borrow `Y_m` in unchanged block order for the later GCM tag calculation.
    #[must_use]
    pub(in crate::aead::gcm) fn as_block(&self) -> &[u8; BLOCK_BYTES] {
        self.0.as_block()
    }

    /// Transfer `S` into the next zeroizing GCM owner without leaving a second copy.
    #[must_use]
    pub(in crate::aead::gcm) fn into_block(self) -> [u8; BLOCK_BYTES] {
        self.0.into_block()
    }
}

#[cfg(test)]
mod unit {
    use super::{Ghash, HashSubkey};

    /// Standard-derived first-iteration evidence from SP 800-38D §6.4 Algorithm 2 and the
    /// published operands in NIST `AES_GCM.pdf`, GCM-AES128 Example 2.
    ///
    /// Since `Y_0` is zero, `Y_1 = X_1 • H`. NIST does not publish this individual intermediate,
    /// so the expected value retains the standard-derived classification recorded in the vector
    /// provenance file.
    #[test]
    fn first_update_matches_the_derived_first_field_product() {
        let hash_subkey = HashSubkey::new([
            0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5,
            0x3b, 0x78,
        ]);
        let first_ciphertext_block = [
            0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
            0xd4, 0x9c,
        ];
        let expected_y_1 = [
            0x59, 0xed, 0x3f, 0x2b, 0xb1, 0xa0, 0xaa, 0xa0, 0x7c, 0x9f, 0x56, 0xc6, 0xa5, 0x04,
            0x64, 0x7b,
        ];
        let mut ghash = Ghash::new(hash_subkey);

        ghash.update_block(&first_ciphertext_block);

        assert_eq!(ghash.finalize().as_block(), &expected_y_1);
    }

    /// Published complete-GHASH evidence from NIST `AES_GCM.pdf`, GCM-AES128 Example 2.
    ///
    /// With empty AAD and 512 ciphertext bits, SP 800-38D Algorithm 4 step 4 supplies the four
    /// published ciphertext blocks followed by `[0]_64 || [512]_64`. NIST publishes the final
    /// GHASH result as `S = 7f1b...4eac`.
    #[test]
    fn example_two_reaches_nists_published_s_value() {
        let hash_subkey = HashSubkey::new([
            0xb8, 0x3b, 0x53, 0x37, 0x08, 0xbf, 0x53, 0x5d, 0x0a, 0xa6, 0xe5, 0x29, 0x80, 0xd5,
            0x3b, 0x78,
        ]);
        let input_blocks = [
            [
                0x42, 0x83, 0x1e, 0xc2, 0x21, 0x77, 0x74, 0x24, 0x4b, 0x72, 0x21, 0xb7, 0x84, 0xd0,
                0xd4, 0x9c,
            ],
            [
                0xe3, 0xaa, 0x21, 0x2f, 0x2c, 0x02, 0xa4, 0xe0, 0x35, 0xc1, 0x7e, 0x23, 0x29, 0xac,
                0xa1, 0x2e,
            ],
            [
                0x21, 0xd5, 0x14, 0xb2, 0x54, 0x66, 0x93, 0x1c, 0x7d, 0x8f, 0x6a, 0x5a, 0xac, 0x84,
                0xaa, 0x05,
            ],
            [
                0x1b, 0xa3, 0x0b, 0x39, 0x6a, 0x0a, 0xac, 0x97, 0x3d, 0x58, 0xe0, 0x91, 0x47, 0x3f,
                0x59, 0x85,
            ],
            [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x02, 0x00,
            ],
        ];
        let expected_s = [
            0x7f, 0x1b, 0x32, 0xb8, 0x1b, 0x82, 0x0d, 0x02, 0x61, 0x4f, 0x88, 0x95, 0xac, 0x1d,
            0x4e, 0xac,
        ];
        let mut ghash = Ghash::new(hash_subkey);

        for block in &input_blocks {
            ghash.update_block(block);
        }

        assert_eq!(ghash.finalize().as_block(), &expected_s);
    }
}
