//! The crate's checked-construction error types.
//!
//! Hand-rolled (no `thiserror`) to keep `bnb` dependency-light — protocol crates
//! depend on `bnb` *instead of* a stack of external helpers, so `bnb` itself
//! stays lean.

use core::fmt;

/// Errors from checked construction.
///
/// Currently the only fallible operation is `UInt::try_new` (and the `TryFrom`
/// impls built on it). Decoding never fails: the codec is dual-use, so unknown
/// values are preserved via a `#[catch_all]` variant rather than rejected, and
/// field access masks rather than validates.
///
/// # Examples
///
/// ```
/// use bnb::{u4, WidthError};
///
/// let err = u4::try_new(20).unwrap_err(); // 20 doesn't fit in 4 bits
/// assert_eq!(err, WidthError::ValueTooLarge { value: 20, bits: 4 });
/// assert_eq!(err.to_string(), "value 20 does not fit in 4 bits");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WidthError {
    /// A value did not fit in the target integer's bit width.
    ValueTooLarge {
        /// The offending value.
        value: u128,
        /// The maximum number of bits available.
        bits: u32,
    },
}

impl fmt::Display for WidthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WidthError::ValueTooLarge { value, bits } => {
                write!(f, "value {value} does not fit in {bits} bits")
            }
        }
    }
}

impl core::error::Error for WidthError {}

/// The error returned by the `TryFrom<uN>` impl that `#[derive(BitEnum)]`
/// generates for a **byte-aligned enum without a `#[catch_all]` variant**: the
/// value matched no known discriminant.
///
/// A catch-all enum is total, so its primitive→enum conversion is an infallible
/// `From` and never produces this. This mirrors `num_enum`'s
/// `TryFromPrimitiveError`. Decoding through the codec / `Bits::from_bits` is
/// unaffected — that path stays permissive (dual-use); this is only for the
/// caller who opts into a checked conversion.
///
/// # Examples
///
/// ```
/// #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
/// #[bit_enum(u8, closed)]
/// #[repr(u8)]
/// enum Direction { Request = 1, Reply = 2 }
///
/// let err = Direction::try_from(9u8).unwrap_err(); // no variant for 9
/// assert_eq!(err.value, 9);
/// assert_eq!(err.type_name, "Direction");
/// assert_eq!(err.to_string(), "Direction has no variant for discriminant 9");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnknownDiscriminant {
    /// The unrecognized value.
    pub value: u128,
    /// The name of the enum that rejected it.
    pub type_name: &'static str,
}

impl UnknownDiscriminant {
    /// Builds the error for a `value` that matched no discriminant of the enum
    /// named `type_name`. Called by `#[derive(BitEnum)]`-generated `TryFrom` impls.
    #[must_use]
    pub const fn new(value: u128, type_name: &'static str) -> Self {
        Self { value, type_name }
    }
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

impl core::error::Error for UnknownDiscriminant {}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn value_too_large_carries_value_and_width() {
        let e = WidthError::ValueTooLarge { value: 20, bits: 4 };
        assert_eq!(e, WidthError::ValueTooLarge { value: 20, bits: 4 });
    }

    #[test]
    fn value_too_large_display() {
        let e = WidthError::ValueTooLarge { value: 20, bits: 4 };
        assert_eq!(e.to_string(), "value 20 does not fit in 4 bits");
    }

    #[test]
    fn unknown_discriminant_carries_value_and_name() {
        let e = UnknownDiscriminant {
            value: 9,
            type_name: "Direction",
        };
        assert_eq!(e.value, 9);
        assert_eq!(e.type_name, "Direction");
    }

    #[test]
    fn unknown_discriminant_display() {
        let e = UnknownDiscriminant {
            value: 9,
            type_name: "Direction",
        };
        assert_eq!(e.to_string(), "Direction has no variant for discriminant 9");
    }
}
