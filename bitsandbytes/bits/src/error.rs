//! The crate's error type and result alias.
//!
//! Hand-rolled (no `thiserror`) to keep `bits` dependency-light — the long-term
//! goal is for protocol crates to depend on `bits` *instead of* a stack of
//! external helpers, so `bits` itself stays lean.

use core::fmt;

/// Errors from checked construction.
///
/// Currently the only fallible operation is `UInt::try_new` (and the `TryFrom`
/// impls built on it). Decoding never fails: the codec is dual-use, so unknown
/// values are preserved as `Custom`/catch-all rather than rejected, and field
/// access masks rather than validates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A value did not fit in the target integer's bit width.
    ValueTooLarge {
        /// The offending value.
        value: u128,
        /// The maximum number of bits available.
        bits: u32,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ValueTooLarge { value, bits } => {
                write!(f, "value {value} does not fit in {bits} bits")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A `Result` specialized to [`Error`].
pub type Result<T> = core::result::Result<T, Error>;
