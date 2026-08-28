//! Supported byte-length limits for `AEAD_CHACHA20_POLY1305`.
//!
//! ## Standards ownership
//!
//! RFC 8439 §2.8 encrypts with a 32-bit block counter starting at one, so a plaintext may occupy
//! at most `2^32 - 1` blocks of 64 bytes: `(2^32 - 1) * 64 = 2^38 - 64` bytes. The AAD and
//! ciphertext lengths are encoded as 64-bit integers, which bounds the AAD to `2^64 - 1` bytes.
//! Both limits are checked before any transformation so failure leaves no partial output.

use crate::{CryptoError, Result};

/// Largest plaintext or ciphertext the counter can cover from block one.
pub(super) const MAX_PAYLOAD_BYTES: u64 = ((1_u64 << 32) - 1) * 64;

fn as_u64(byte_length: usize) -> Result<u64> {
    u64::try_from(byte_length).map_err(|_| CryptoError::MessageTooLong)
}

/// Reject lengths the construction cannot encrypt or encode.
///
/// # Errors
///
/// Returns [`CryptoError::MessageTooLong`] when the payload exceeds `2^38 - 64` bytes or the AAD
/// length cannot be represented in 64 bits.
pub(super) fn validate_input_lengths(associated_data_len: usize, payload_len: usize) -> Result<()> {
    as_u64(associated_data_len)?;
    if as_u64(payload_len)? > MAX_PAYLOAD_BYTES {
        return Err(CryptoError::MessageTooLong);
    }
    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn payload_limit_is_the_last_counter_block() {
        assert_eq!(MAX_PAYLOAD_BYTES, (1 << 38) - 64);
        assert!(validate_input_lengths(0, 0).is_ok());
        #[cfg(target_pointer_width = "64")]
        {
            let limit = usize::try_from(MAX_PAYLOAD_BYTES).unwrap();
            assert!(validate_input_lengths(0, limit).is_ok());
            assert_eq!(
                validate_input_lengths(0, limit + 1),
                Err(CryptoError::MessageTooLong)
            );
        }
    }
}
