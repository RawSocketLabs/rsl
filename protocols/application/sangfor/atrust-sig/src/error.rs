//! Why a primitive could not be computed.

// --- Workspace dependencies ---
use crypto::CryptoError;
use thiserror::Error as ThisError;

/// Why a primitive could not be computed.
#[derive(Clone, Debug, ThisError, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A value the server sent is not in the form the rule needs.
    #[error("malformed {0}")]
    Malformed(&'static str),
    /// A cryptographic primitive refused its input.
    #[error("cryptographic failure")]
    Crypto(#[from] CryptoError),
}
