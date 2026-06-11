//! The crate's error type and result alias.
//!
//! Hand-rolled (no `thiserror`) to keep `bits` dependency-light — the long-term
//! goal is for protocol crates to depend on `bits` *instead of* a stack of
//! external helpers, so `bits` itself stays lean.

use core::fmt;

/// Errors from checked construction and decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A value did not fit in the target field/integer width.
    ValueTooLarge {
        /// The offending value.
        value: u128,
        /// The maximum number of bits available.
        bits: u32,
    },

    /// A bit-enum value had no matching variant and the enum has no catch-all.
    UnknownVariant {
        /// The discriminant that did not match.
        value: u128,
        /// The name of the enum type.
        type_name: &'static str,
    },

    /// Not enough bytes were available to decode a value.
    Truncated {
        /// How many bytes were needed.
        needed: usize,
        /// How many were available.
        available: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ValueTooLarge { value, bits } => {
                write!(f, "value {value} does not fit in {bits} bits")
            }
            Error::UnknownVariant { value, type_name } => {
                write!(f, "no {type_name} variant for discriminant {value}")
            }
            Error::Truncated { needed, available } => {
                write!(f, "truncated: needed {needed} bytes, had {available}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// A `Result` specialized to [`Error`].
pub type Result<T> = core::result::Result<T, Error>;
