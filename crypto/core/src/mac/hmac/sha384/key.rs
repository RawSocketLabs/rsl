//! HMAC-SHA-384 key normalization and pad derivation.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 198-1 §2.3 and §4, Table 1, steps 1–3][fips-198-1] define `K0`: a key exactly as
//! wide as the hash input block. A 128-byte SHA-384 key is used directly, a shorter key is padded
//! on the right with zero bytes, and a longer key is first hashed to 48 bytes and then padded on
//! the right. Table 1 steps 4 and 7 XOR that `K0` with the repeated inner and outer pad bytes.
//!
//! `K0` and both derived blocks remain in [`SecretBytes`] so their formatting is redacted and
//! their owned storage is zeroized on drop. The temporary digest of a long key is also explicitly
//! zeroized after it is copied into `K0`.
//!
//! [fips-198-1]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf

use zeroize::Zeroize;

use crate::{Result, SecretBytes, digest::sha2::sha384::Sha384};

/// SHA-384's input block length, called `B` by FIPS 198-1 §2.3.
pub(super) const KEY_BLOCK_LEN: usize = 128;

/// Byte repeated across the inner pad, `ipad`, by FIPS 198-1 §2.3.
const INNER_PAD_BYTE: u8 = 0x36;

/// Byte repeated across the outer pad, `opad`, by FIPS 198-1 §2.3.
const OUTER_PAD_BYTE: u8 = 0x5c;

/// The secret, exactly-one-block representation `K0` from FIPS 198-1.
pub(super) struct NormalizedKey(SecretBytes<KEY_BLOCK_LEN>);

impl NormalizedKey {
    /// Apply FIPS 198-1 Table 1 steps 1–3 to arbitrary key bytes.
    pub(super) fn from_key(key: &[u8]) -> Result<Self> {
        let mut normalized = [0_u8; KEY_BLOCK_LEN];

        if key.len() > KEY_BLOCK_LEN {
            let mut hashed_key = Sha384::digest(key)?.into_bytes();
            normalized[..hashed_key.len()].copy_from_slice(&hashed_key);
            hashed_key.zeroize();
        } else {
            normalized[..key.len()].copy_from_slice(key);
        }

        Ok(Self(SecretBytes::new(normalized)))
    }

    /// Consume `K0` and derive the complete inner and outer blocks from Table 1 steps 4 and 7.
    pub(super) fn into_padded_blocks(
        self,
    ) -> (SecretBytes<KEY_BLOCK_LEN>, SecretBytes<KEY_BLOCK_LEN>) {
        let mut inner = [0_u8; KEY_BLOCK_LEN];
        let mut outer = [0_u8; KEY_BLOCK_LEN];

        for (index, key_byte) in self.0.expose_secret().iter().copied().enumerate() {
            inner[index] = key_byte ^ INNER_PAD_BYTE;
            outer[index] = key_byte ^ OUTER_PAD_BYTE;
        }

        (SecretBytes::new(inner), SecretBytes::new(outer))
    }

    /// Expose `K0` only to focused white-box tests of this standards layer.
    #[cfg(test)]
    fn expose_for_test(&self) -> &[u8; KEY_BLOCK_LEN] {
        self.0.expose_secret()
    }
}

#[cfg(test)]
mod unit {
    use super::{INNER_PAD_BYTE, KEY_BLOCK_LEN, NormalizedKey, OUTER_PAD_BYTE};

    /// Standard-derived evidence for FIPS 198-1 §4, Table 1, step 3.
    #[test]
    fn short_keys_are_copied_then_padded_on_the_right_with_zeroes() {
        let key = b"key";
        let normalized = NormalizedKey::from_key(key).expect("a short key is valid input");

        assert_eq!(&normalized.expose_for_test()[..key.len()], key);
        assert!(
            normalized.expose_for_test()[key.len()..]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    /// Standard-derived evidence for FIPS 198-1 §4, Table 1, step 1.
    #[test]
    fn one_block_key_is_used_without_hashing_or_padding() {
        let key = core::array::from_fn(|index| {
            u8::try_from(index).expect("every SHA-384 block index fits in u8")
        });
        let normalized = NormalizedKey::from_key(&key).expect("a one-block key is valid input");

        assert_eq!(normalized.expose_for_test(), &key);
    }

    /// Standard-derived evidence for FIPS 198-1 §4, Table 1, step 2, with the digest of the
    /// over-long key taken from the development-only `sha2` oracle (differential evidence).
    #[test]
    fn longer_than_block_key_is_hashed_then_padded_on_the_right() {
        use sha2::Digest as _;
        let key = [0xaa; KEY_BLOCK_LEN + 1];
        let expected_hash = sha2::Sha384::digest(key);
        let normalized = NormalizedKey::from_key(&key).expect("the test key fits SHA-384");

        assert_eq!(
            &normalized.expose_for_test()[..expected_hash.len()],
            expected_hash.as_slice()
        );
        assert!(
            normalized.expose_for_test()[expected_hash.len()..]
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    /// Standard-derived evidence for FIPS 198-1 §4, Table 1, steps 4 and 7.
    #[test]
    fn every_byte_of_both_pad_blocks_is_an_explicit_xor_with_k0() {
        let key = [0xa5; KEY_BLOCK_LEN];
        let normalized = NormalizedKey::from_key(&key).expect("a one-block key is valid input");
        let (inner, outer) = normalized.into_padded_blocks();

        assert!(
            inner
                .expose_secret()
                .iter()
                .all(|byte| *byte == 0xa5 ^ INNER_PAD_BYTE)
        );
        assert!(
            outer
                .expose_secret()
                .iter()
                .all(|byte| *byte == 0xa5 ^ OUTER_PAD_BYTE)
        );
    }
}
