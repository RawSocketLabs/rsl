//! The crate error type.

use thiserror::Error;

/// An error decoding or constructing a DNS message.
///
/// Decode errors from the `bnb` codec are wrapped as [`Codec`](DnsError::Codec) (they
/// already carry a bit offset and field name); the remaining variants cover
/// construction- and resolution-side problems.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DnsError {
    /// A wire decode/encode error from the `bnb` codec (carries offset + field).
    #[error("codec error: {0}")]
    Codec(#[from] bnb::BitError),

    /// A compression pointer that could not be followed (loop, or out of range).
    #[error("bad compression pointer: {0}")]
    BadPointer(String),

    /// A value that cannot be represented on the wire (e.g. a label over 63 bytes, a
    /// character-string over 255). Construction-side only — the parser stays permissive.
    #[error("not representable on the wire: {0}")]
    NotRepresentable(String),

    /// A semantic validation failure (construction-side; never raised by the parser).
    #[error("validation error: {0}")]
    Validation(String),
}

/// A `Result` specialized to [`DnsError`].
pub type Result<T> = core::result::Result<T, DnsError>;
