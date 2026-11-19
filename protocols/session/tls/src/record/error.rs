//! Failures while protecting or opening a record.

// --- Workspace dependencies ---
use crypto::CryptoError;
use thiserror::Error;

/// Why a record could not be protected or opened.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RecordError {
    /// The plaintext is too long for its protected fragment to fit a record's 16-bit length.
    #[error("record is longer than a 16-bit length can describe")]
    PlaintextTooLong,
    /// The fragment is shorter than the explicit nonce and tag every protected record has.
    #[error("protected record is shorter than its nonce and tag")]
    Truncated,
    /// The record did not authenticate: it was altered, or opened with the wrong key,
    /// sequence number, type or version.
    #[error("record failed authentication")]
    BadRecordMac,
    /// A record, or the plaintext inside it, is longer than the protocol allows. A receiver
    /// answers this with `record_overflow`.
    #[error("record is longer than the protocol allows")]
    RecordOverflow,
    /// The next record's sequence number would wrap, so this key can protect no more records.
    #[error("record sequence number is exhausted")]
    SequenceExhausted,
    /// The sending key has reached its AES-GCM usage budget. Establish a fresh connection.
    #[error("TLS sending key exhausted its AES-GCM record budget; reconnect")]
    KeyUsageLimit,
    /// The cipher failed for another reason.
    #[error("record cipher failed")]
    Crypto(#[source] CryptoError),
}

impl From<CryptoError> for RecordError {
    fn from(error: CryptoError) -> Self {
        match error {
            CryptoError::AuthenticationFailed => Self::BadRecordMac,
            other => Self::Crypto(other),
        }
    }
}
