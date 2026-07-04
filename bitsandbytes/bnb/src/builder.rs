//! Support types for `#[derive(BitsBuilder)]` and `#[bin]`.

use alloc::string::String;
use core::fmt;

/// The error a generated builder's `build()` returns.
///
/// Two cases:
/// - [`MissingField`](BuilderError::MissingField) — a **required** field was
///   never set. "Required" is the default; a field is only optional if it carries
///   `#[builder(default)]` (or `#[builder(default = expr)]`). This is what lets the
///   builder *call out* an unset bit/byte instead of silently defaulting it to
///   zero (the gap in the infix `with_*` API).
/// - [`Invalid`](BuilderError::Invalid) — a `#[bin(validate = …)]` soundness
///   check rejected the built value. The string is the validator's error,
///   stringified, so any `Display` error type composes without coupling the
///   builder to a protocol-specific error type.
///
/// # Examples
///
/// ```
/// use bnb::{bitfield, u4, BitsBuilder, BuilderError};
///
/// #[bitfield(u8, bits = msb)]
/// #[derive(BitsBuilder, Clone, Copy, Debug)]
/// struct Nibbles { hi: u4, lo: u4 }
///
/// let err = Nibbles::builder().hi(u4::new(0xA)).build().unwrap_err();
/// assert_eq!(err, BuilderError::MissingField("lo"));
/// assert_eq!(err.field(), Some("lo"));
/// assert_eq!(err.to_string(), "required field `lo` was not set");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuilderError {
    /// A required field was not set; carries the field name.
    MissingField(&'static str),
    /// A soundness validator rejected the value; carries its message.
    Invalid(String),
}

impl BuilderError {
    /// Constructs the "required field not set" error for `field`.
    pub fn missing_field(field: &'static str) -> Self {
        Self::MissingField(field)
    }

    /// Constructs a soundness-failure error from a validator's message.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// The name of the field that was not set, or `None` for an
    /// [`Invalid`](BuilderError::Invalid) error.
    pub fn field(&self) -> Option<&'static str> {
        match self {
            Self::MissingField(field) => Some(field),
            Self::Invalid(_) => None,
        }
    }
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "required field `{field}` was not set"),
            Self::Invalid(message) => write!(f, "soundness check failed: {message}"),
        }
    }
}

impl core::error::Error for BuilderError {}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn missing_field_constructor_and_accessor() {
        let e = BuilderError::missing_field("flags");
        assert_eq!(e, BuilderError::MissingField("flags"));
        assert_eq!(e.field(), Some("flags"));
    }

    #[test]
    fn missing_field_display() {
        assert_eq!(
            BuilderError::missing_field("flags").to_string(),
            "required field `flags` was not set",
        );
    }

    #[test]
    fn invalid_constructor_accepts_any_display() {
        // `impl Into<String>` — a &str and a String both compose.
        let from_str = BuilderError::invalid("bad");
        let from_string = BuilderError::invalid(String::from("bad"));
        assert_eq!(from_str, from_string);
        assert_eq!(from_str, BuilderError::Invalid("bad".into()));
    }

    #[test]
    fn invalid_has_no_field_name() {
        assert_eq!(BuilderError::invalid("nope").field(), None);
    }

    #[test]
    fn invalid_display_wraps_the_message() {
        assert_eq!(
            BuilderError::invalid("kind 0 is reserved").to_string(),
            "soundness check failed: kind 0 is reserved",
        );
    }
}
