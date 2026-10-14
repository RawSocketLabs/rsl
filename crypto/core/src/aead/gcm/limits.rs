//! Supported byte-length limits for this GCM construction.
//!
//! ## Standards ownership
//!
//! [NIST SP 800-38D §5.2.1.1][sp-800-38d] requires:
//!
//! - `len(P) <= 2^39 - 256` bits for plaintext;
//! - `len(A) <= 2^64 - 1` bits for additional authenticated data; and
//! - `1 <= len(IV) <= 2^64 - 1` bits for the initialization vector.
//!
//! Section 5.2.2 requires authenticated decryption to support the same ciphertext, AAD, and IV
//! lengths as authenticated encryption. Because this implementation accepts byte strings rather
//! than arbitrary bit strings, the largest supported whole-byte AAD is
//! `floor((2^64 - 1) / 8) = 2^61 - 1` bytes. The payload limit is exactly `2^36 - 32` bytes.
//!
//! This module validates lengths before encryption or decryption starts. The IV limit does not
//! appear here because [`super::setup::GcmIv96`] deliberately restricts the construction to the
//! 96-bit IV length recommended by §5.2.1.1 and makes every other length unrepresentable.
//!
//! [sp-800-38d]: https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "private GCM input limits land before complete encryption composition"
    )
)]

use crate::{CryptoError, Result};

/// Largest byte-aligned plaintext or ciphertext accepted by SP 800-38D §5.2.1.1.
const MAX_PAYLOAD_BYTES: u64 = (1_u64 << 36) - 32;

/// Largest byte-aligned AAD whose bit length is at most `2^64 - 1`.
const MAX_ASSOCIATED_DATA_BYTES: u64 = u64::MAX / 8_u64;

/// Convert a platform byte length to the stable integer domain used by the limits above.
fn as_u64(byte_length: usize) -> Result<u64> {
    u64::try_from(byte_length).map_err(|_| CryptoError::MessageTooLong)
}

/// Validate the AAD and plaintext/ciphertext lengths for one GCM invocation.
///
/// This check applies equally to encryption and decryption, as required by SP 800-38D §5.2.2.
/// It accepts lengths rather than slices so boundary behavior can be tested without attempting
/// enormous allocations.
///
/// # Errors
///
/// Returns [`CryptoError::MessageTooLong`] if either length exceeds its standard-defined bound or
/// cannot be represented as a `u64` byte count.
pub(super) fn validate_input_lengths(associated_data_len: usize, payload_len: usize) -> Result<()> {
    let associated_data_len = as_u64(associated_data_len)?;
    let payload_len = as_u64(payload_len)?;

    if associated_data_len > MAX_ASSOCIATED_DATA_BYTES || payload_len > MAX_PAYLOAD_BYTES {
        return Err(CryptoError::MessageTooLong);
    }

    Ok(())
}

#[cfg(test)]
mod unit {
    use super::{
        CryptoError, MAX_ASSOCIATED_DATA_BYTES, MAX_PAYLOAD_BYTES, validate_input_lengths,
    };

    #[test]
    fn ordinary_and_empty_inputs_are_supported() {
        assert_eq!(validate_input_lengths(0, 0), Ok(()));
        assert_eq!(validate_input_lengths(20, 60), Ok(()));
    }

    /// Standard-derived boundary evidence for `len(P) <= 2^39 - 256` bits.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn payload_limit_accepts_its_boundary_and_rejects_the_next_byte() {
        let boundary =
            usize::try_from(MAX_PAYLOAD_BYTES).expect("the 64-bit target fits the bound");

        assert_eq!(validate_input_lengths(0, boundary), Ok(()));
        assert_eq!(
            validate_input_lengths(0, boundary + 1),
            Err(CryptoError::MessageTooLong)
        );
    }

    /// Standard-derived whole-byte boundary evidence for `len(A) <= 2^64 - 1` bits.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn associated_data_limit_accepts_its_boundary_and_rejects_the_next_byte() {
        let boundary = usize::try_from(MAX_ASSOCIATED_DATA_BYTES)
            .expect("the 64-bit target fits the whole-byte bound");

        assert_eq!(validate_input_lengths(boundary, 0), Ok(()));
        assert_eq!(
            validate_input_lengths(boundary + 1, 0),
            Err(CryptoError::MessageTooLong)
        );
    }

    #[test]
    fn payload_and_aad_limits_are_independent() {
        assert_eq!(validate_input_lengths(1, 0), Ok(()));
        assert_eq!(validate_input_lengths(0, 1), Ok(()));
    }
}
