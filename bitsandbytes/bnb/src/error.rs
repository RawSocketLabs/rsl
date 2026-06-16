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

/// The error returned by the `TryFrom<uN>` impl that `#[derive(BitEnum)]`
/// generates for a **byte-aligned enum without a `#[catch_all]` variant**: the
/// value matched no known discriminant.
///
/// A catch-all enum is total, so its primitive→enum conversion is an infallible
/// `From` and never produces this. This mirrors `num_enum`'s
/// `TryFromPrimitiveError`. Decoding through `binrw`/`Bits::from_bits` is
/// unaffected — that path stays permissive (dual-use); this is only for the
/// caller who opts into a checked conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownDiscriminant {
    /// The unrecognized value.
    pub value: u128,
    /// The name of the enum that rejected it.
    pub type_name: &'static str,
}

impl fmt::Display for UnknownDiscriminant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} has no variant for discriminant {}",
            self.type_name, self.value
        )
    }
}

impl std::error::Error for UnknownDiscriminant {}
