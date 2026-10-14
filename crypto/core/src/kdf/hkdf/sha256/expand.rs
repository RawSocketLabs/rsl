//! HKDF-SHA-256 Expand recurrence and output-length enforcement.
//!
//! ## Standards ownership
//!
//! [RFC 5869 §2.3][rfc-5869] defines `T(0)` as empty and each subsequent 32-byte block as
//! `HMAC-SHA-256(PRK, T(i-1) || info || i)`, where `i` is one octet. The output is the first `L`
//! bytes of the concatenated blocks. The RFC limits `L` to `255 * HashLen`, which is 8,160 bytes
//! for SHA-256; this module checks that limit before writing any caller output.
//!
//! The previous `T` block is secret keying material. It remains in [`SecretBytes`] between
//! iterations and is zeroized when replaced or dropped. Caller-owned output is also secret
//! keying material, so the caller is responsible for its eventual destruction.
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

use crate::{CryptoError, Result, SecretBytes, kdf::KeyExpander, mac::hmac::sha256::HmacSha256};

use super::{HASH_LEN, HkdfSha256Prk};

/// Maximum RFC 5869 output: 255 one-octet-indexed SHA-256 blocks.
const MAX_OUTPUT_LEN: usize = 255 * HASH_LEN;

impl HkdfSha256Prk {
    /// The maximum HKDF-SHA-256 output length in bytes.
    pub const MAX_OUTPUT_LEN: usize = MAX_OUTPUT_LEN;

    /// Expand this PRK and context into exactly `output.len()` keying-material bytes.
    ///
    /// `info` is optional at the RFC layer and is represented by an empty slice when absent. A
    /// protocol should encode its labels, identities, algorithm choices, transcript hashes, and
    /// other context into unambiguous bytes before this call.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::OutputTooLong`] before modifying `output` when its length exceeds
    /// 8,160 bytes. Returns [`CryptoError::MessageTooLong`] if `info` cannot fit in an underlying
    /// HMAC-SHA-256 message.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::kdf::hkdf::sha256::extract;
    ///
    /// let prk = extract(Some(b"salt"), b"input keying material")?;
    /// let mut client_key = [0_u8; 16];
    /// let mut server_key = [0_u8; 16];
    /// prk.expand(b"client encryption key", &mut client_key)?;
    /// prk.expand(b"server encryption key", &mut server_key)?;
    /// assert_ne!(client_key, server_key);
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    pub fn expand(&self, info: &[u8], output: &mut [u8]) -> Result<()> {
        if output.len() > MAX_OUTPUT_LEN {
            return Err(CryptoError::OutputTooLong);
        }

        let block_count = output.len().div_ceil(HASH_LEN);
        let final_counter = u8::try_from(block_count).map_err(|_| CryptoError::OutputTooLong)?;
        let mut previous = SecretBytes::new([0_u8; HASH_LEN]);
        let mut previous_len = 0;
        let mut written = 0;

        for counter in 1..=final_counter {
            let mut hmac = HmacSha256::new(self.expose_secret())?;
            hmac.update(&previous.expose_secret()[..previous_len])?;
            hmac.update(info)?;
            hmac.update([counter])?;

            previous = SecretBytes::new(hmac.finalize().into_bytes());
            previous_len = HASH_LEN;

            let remaining = output.len() - written;
            let copied = remaining.min(HASH_LEN);
            output[written..written + copied].copy_from_slice(&previous.expose_secret()[..copied]);
            written += copied;
        }

        Ok(())
    }
}

impl KeyExpander for HkdfSha256Prk {
    fn expand(&self, info: &[u8], output: &mut [u8]) -> Result<()> {
        self.expand(info, output)
    }
}

#[cfg(test)]
mod unit {
    use alloc::vec;

    use super::{HASH_LEN, HkdfSha256Prk, MAX_OUTPUT_LEN};
    use crate::{CryptoError, kdf::hkdf::sha256::extract};

    /// Published output evidence from RFC 5869 Appendix A.1.
    #[test]
    fn test_case_1_expands_across_a_partial_second_block() {
        let ikm = [0x0b; 22];
        let salt: [u8; 13] = core::array::from_fn(|index| {
            u8::try_from(index).expect("RFC 5869 Test Case 1 salt indices fit u8")
        });
        let info: [u8; 10] = core::array::from_fn(|index| {
            0xf0_u8.wrapping_add(
                u8::try_from(index).expect("RFC 5869 Test Case 1 info indices fit u8"),
            )
        });
        let expected = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
        ];
        let prk = extract(Some(&salt), &ikm).expect("the RFC fixture is within HMAC limits");
        let mut output = [0_u8; 42];

        prk.expand(&info, &mut output)
            .expect("42 bytes are within the RFC output limit");

        assert_eq!(output, expected);
    }

    /// Boundary evidence from RFC 5869 §2.3's `L` definition.
    #[test]
    fn zero_length_output_performs_no_block_and_succeeds() {
        let prk = test_prk();
        let mut output = [];

        assert_eq!(prk.expand(b"context", &mut output), Ok(()));
    }

    /// Standard-derived recurrence evidence at the first complete-block boundary.
    #[test]
    fn byte_33_comes_from_t2_not_t1() {
        let prk = test_prk();
        let mut first_block = [0_u8; HASH_LEN];
        let mut block_plus_one = [0_u8; HASH_LEN + 1];

        prk.expand(b"context", &mut first_block)
            .expect("one block is permitted");
        prk.expand(b"context", &mut block_plus_one)
            .expect("one block plus one byte is permitted");

        assert_eq!(&block_plus_one[..HASH_LEN], &first_block);
        assert_ne!(block_plus_one[HASH_LEN], first_block[0]);
    }

    /// Boundary evidence that RFC 5869's length limit is checked before output mutation.
    #[test]
    fn output_above_255_blocks_is_rejected_without_mutation() {
        let prk = test_prk();
        let mut output = vec![0xa5; MAX_OUTPUT_LEN + 1];

        assert_eq!(
            prk.expand(b"context", &mut output),
            Err(CryptoError::OutputTooLong)
        );
        assert!(output.iter().all(|byte| *byte == 0xa5));
    }

    /// Construct fixed secret test input without exposing a public raw-PRK constructor.
    fn test_prk() -> HkdfSha256Prk {
        extract(Some(b"salt"), b"input keying material").expect("the fixture is short")
    }
}
