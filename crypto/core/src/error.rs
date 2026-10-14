//! Security-relevant error categories shared by primitive implementations.
//!
//! Errors intentionally avoid carrying secret-dependent diagnostic detail. Protocol code can
//! distinguish public configuration problems such as [`CryptoError::InvalidLength`] from a
//! uniform authentication rejection, without learning which tag byte or protected field failed.
//!
//! # Examples
//!
//! ```
//! use rsl_crypto::{CryptoError, aead::gcm::Aes128GcmTag};
//!
//! let short_wire_tag = [0_u8; 15];
//! assert_eq!(
//!     Aes128GcmTag::try_from(short_wire_tag.as_slice()),
//!     Err(CryptoError::InvalidLength {
//!         name: "AES-128-GCM tag",
//!         expected: 16,
//!         actual: 15,
//!     }),
//! );
//! ```
//!
//! Display text is suitable for a public diagnostic but deliberately omits secret comparison
//! details:
//!
//! ```
//! use rsl_crypto::CryptoError;
//!
//! assert_eq!(
//!     CryptoError::AuthenticationFailed.to_string(),
//!     "authentication failed",
//! );
//! ```

use core::fmt;

/// A result produced by this crate, using its security-oriented [`CryptoError`] categories.
pub type Result<T> = core::result::Result<T, CryptoError>;

/// A cryptographic construction or input failed.
///
/// Concrete algorithms may add more specific internal errors, but protocol consumers should be
/// able to handle these security-relevant categories without inspecting secret-dependent detail.
/// See the [`error` module](crate::error) for an exact-length example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CryptoError {
    /// An authentication tag did not verify.
    AuthenticationFailed,
    /// A named input had the wrong byte length.
    InvalidLength {
        /// The input being measured.
        name: &'static str,
        /// The required byte length.
        expected: usize,
        /// The supplied byte length.
        actual: usize,
    },
    /// A key was the correct length but not a valid key for the algorithm.
    InvalidKey,
    /// A public key was malformed, disallowed, or produced a rejected agreement result.
    InvalidPublicKey,
    /// A signature was malformed or did not verify.
    InvalidSignature,
    /// A derivation was asked to produce more output than its construction permits.
    OutputTooLong,
    /// A message exceeded the length representable by its cryptographic construction.
    MessageTooLong,
    /// A nonce, block, or record counter would repeat or wrap.
    CounterExhausted,
    /// An incremental state was invalidated by an earlier output or processing failure.
    StateInvalidated,
    /// The configured randomness source could not fill the requested output.
    EntropyUnavailable,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationFailed => f.write_str("authentication failed"),
            Self::InvalidLength {
                name,
                expected,
                actual,
            } => write!(
                f,
                "invalid {name} length: expected {expected} bytes, got {actual}"
            ),
            Self::InvalidKey => f.write_str("invalid cryptographic key"),
            Self::InvalidPublicKey => f.write_str("invalid public key"),
            Self::InvalidSignature => f.write_str("invalid signature"),
            Self::OutputTooLong => f.write_str("requested cryptographic output is too long"),
            Self::MessageTooLong => {
                f.write_str("message is too long for this cryptographic construction")
            }
            Self::CounterExhausted => f.write_str("cryptographic counter exhausted"),
            Self::StateInvalidated => {
                f.write_str("cryptographic state was invalidated by an earlier failure")
            }
            Self::EntropyUnavailable => f.write_str("randomness source unavailable"),
        }
    }
}

impl core::error::Error for CryptoError {}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn invalid_length_names_the_failed_boundary() {
        let error = CryptoError::InvalidLength {
            name: "nonce",
            expected: 12,
            actual: 8,
        };

        assert_eq!(
            error.to_string(),
            "invalid nonce length: expected 12 bytes, got 8"
        );
    }

    #[test]
    fn message_length_exhaustion_has_a_stable_public_error() {
        assert_eq!(
            CryptoError::MessageTooLong.to_string(),
            "message is too long for this cryptographic construction"
        );
    }

    #[test]
    fn invalidated_state_has_a_stable_public_error() {
        assert_eq!(
            CryptoError::StateInvalidated.to_string(),
            "cryptographic state was invalidated by an earlier failure"
        );
    }
}
