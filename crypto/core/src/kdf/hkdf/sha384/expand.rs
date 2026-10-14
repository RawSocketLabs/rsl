//! HKDF-SHA-384 Expand recurrence and output-length enforcement.
//!
//! ## Standards ownership
//!
//! [RFC 5869 §2.3][rfc-5869] defines `T(0)` as empty and each subsequent 48-byte block as
//! `HMAC-SHA-384(PRK, T(i-1) || info || i)`, where `i` is one octet. The output is the first `L`
//! bytes of the concatenated blocks. The RFC limits `L` to `255 * HashLen`, which is 12,240 bytes
//! for SHA-384; this module checks that limit before writing any caller output.
//!
//! The previous `T` block is secret keying material. It remains in [`SecretBytes`] between
//! iterations and is zeroized when replaced or dropped. Caller-owned output is also secret
//! keying material, so the caller is responsible for its eventual destruction.
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

use crate::{CryptoError, Result, SecretBytes, kdf::KeyExpander, mac::hmac::sha384::HmacSha384};

use super::{HASH_LEN, HkdfSha384Prk};

/// Maximum RFC 5869 output: 255 one-octet-indexed SHA-384 blocks.
const MAX_OUTPUT_LEN: usize = 255 * HASH_LEN;

impl HkdfSha384Prk {
    /// The maximum HKDF-SHA-384 output length in bytes.
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
    /// 12,240 bytes. Returns [`CryptoError::MessageTooLong`] if `info` cannot fit in an underlying
    /// HMAC-SHA-384 message.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::kdf::hkdf::sha384::extract;
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
            let mut hmac = HmacSha384::new(self.expose_secret())?;
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

impl KeyExpander for HkdfSha384Prk {
    fn expand(&self, info: &[u8], output: &mut [u8]) -> Result<()> {
        self.expand(info, output)
    }
}

#[cfg(test)]
mod unit {
    use alloc::vec;

    use super::{HASH_LEN, HkdfSha384Prk, MAX_OUTPUT_LEN};
    use crate::{CryptoError, kdf::hkdf::sha384::extract};

    /// Standard-derived recurrence evidence from RFC 5869 §2.3 across a partial second block:
    /// `T(1) = HMAC(PRK, info || 0x01)`, `T(2) = HMAC(PRK, T(1) || info || 0x02)`.
    ///
    /// RFC 5869 publishes no SHA-384 vectors; the public Wycheproof suite is the published
    /// evidence for this hash.
    #[test]
    fn sixty_bytes_span_a_partial_second_block_of_the_recurrence() {
        use crate::mac::hmac::sha384::HmacSha384;
        let prk = test_prk();
        let info = b"context";
        let t1 = {
            let mut mac = HmacSha384::new(prk.expose_secret()).unwrap();
            mac.update(info).unwrap();
            mac.update([1]).unwrap();
            mac.finalize().into_bytes()
        };
        let t2 = {
            let mut mac = HmacSha384::new(prk.expose_secret()).unwrap();
            mac.update(t1).unwrap();
            mac.update(info).unwrap();
            mac.update([2]).unwrap();
            mac.finalize().into_bytes()
        };
        let mut output = [0_u8; 60];

        prk.expand(info, &mut output)
            .expect("60 bytes are within the RFC output limit");

        assert_eq!(&output[..HASH_LEN], &t1);
        assert_eq!(&output[HASH_LEN..], &t2[..12]);
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
    fn test_prk() -> HkdfSha384Prk {
        extract(Some(b"salt"), b"input keying material").expect("the fixture is short")
    }
}
