//! Support types for `#[derive(BitsBuilder)]` and `#[bin]`.

use alloc::string::String;
use core::fmt;

/// Replaces enum catch-all aliases with their named variants, preserving discriminants.
///
/// Generated builders run this after resolving fields and before semantic validation.
/// `BitEnum`, ordinary field-based `#[bin]` types, and plain `BitsBuilder` structs
/// implement it automatically. `Vec`, `Option`, and arrays recurse into implementing
/// elements. Opaque types without an implementation are left alone by generated code.
///
/// Implementations must be idempotent, preserve unknown discriminants and collection
/// shape, and change only enum aliases. This is not reserved-field or length repair.
/// Custom codec wrappers may implement this trait to expose their own traversal;
/// field-level custom codecs/mappings and logical-only fields remain opaque.
pub trait NormalizeEnumAliases {
    /// Normalize aliases in place without allocating or performing validation.
    fn normalize_enum_aliases(&mut self);

    /// Consume this value and return it with enum aliases normalized.
    ///
    /// Delegates to [`normalize_enum_aliases`](Self::normalize_enum_aliases): no
    /// cloning, allocation, validation, or reserved-field/length repair is added.
    /// Unknown discriminants, collection shape, and opaque boundaries are retained.
    /// Neither `Clone` nor `Copy` is required. The `Self: Sized` bound leaves the
    /// in-place method callable through `&mut dyn NormalizeEnumAliases`.
    ///
    /// # Examples
    ///
    /// A `Copy` enum can be normalized for comparison without changing the original:
    ///
    /// ```
    /// use bnb::{BitEnum, NormalizeEnumAliases};
    ///
    /// #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    /// #[bit_enum(u8)]
    /// #[repr(u8)]
    /// enum Kind {
    ///     Named = 2,
    ///     #[catch_all]
    ///     Other(u8),
    /// }
    ///
    /// let alias = Kind::Other(2);
    /// assert_eq!(alias.into_normalized_enum_aliases(), Kind::Named);
    /// assert_eq!(alias, Kind::Other(2));
    ///
    /// // Owned collections move; their elements normalize in place.
    /// let values = vec![alias, Kind::Other(99)].into_normalized_enum_aliases();
    /// assert_eq!(values, [Kind::Named, Kind::Other(99)]);
    ///
    /// // Clone explicitly when you need to retain a non-Copy original.
    /// let original = vec![alias];
    /// let normalized = original.clone().into_normalized_enum_aliases();
    /// assert_eq!(original, [Kind::Other(2)]);
    /// assert_eq!(normalized, [Kind::Named]);
    /// ```
    #[must_use]
    fn into_normalized_enum_aliases(mut self) -> Self
    where
        Self: Sized,
    {
        self.normalize_enum_aliases();
        self
    }
}

impl<T: NormalizeEnumAliases> NormalizeEnumAliases for alloc::vec::Vec<T> {
    fn normalize_enum_aliases(&mut self) {
        for value in self {
            value.normalize_enum_aliases();
        }
    }
}

impl<T: NormalizeEnumAliases> NormalizeEnumAliases for Option<T> {
    fn normalize_enum_aliases(&mut self) {
        if let Some(value) = self {
            value.normalize_enum_aliases();
        }
    }
}

impl<T: NormalizeEnumAliases, const N: usize> NormalizeEnumAliases for [T; N] {
    fn normalize_enum_aliases(&mut self) {
        for value in self {
            value.normalize_enum_aliases();
        }
    }
}

// Reachable downstream only through the explicitly unstable `bnb::__private` path.
pub(crate) mod normalization {
    use super::NormalizeEnumAliases;

    /// Concrete-site autoref dispatch for generated code; not a stable API.
    pub struct NormalizeProbe<T: ?Sized>(pub core::marker::PhantomData<fn(&mut T)>);

    /// Autoref fallback keeps opaque fields free of new trait bounds.
    pub trait NormalizeDispatch<T: ?Sized> {
        /// Normalize a supported field, or leave an opaque field untouched.
        fn normalize(self, value: &mut T);
    }

    impl<T: NormalizeEnumAliases + ?Sized> NormalizeDispatch<T> for &NormalizeProbe<T> {
        fn normalize(self, value: &mut T) {
            value.normalize_enum_aliases();
        }
    }

    impl<T: ?Sized> NormalizeDispatch<T> for &&NormalizeProbe<T> {
        fn normalize(self, _: &mut T) {}
    }
}

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
#[non_exhaustive]
pub enum BuilderError {
    /// A required field was not set; carries the field name.
    MissingField(&'static str),
    /// A soundness validator rejected the value; carries its message.
    Invalid(String),
}

impl BuilderError {
    /// Constructs the "required field not set" error for `field`.
    #[must_use]
    pub fn missing_field(field: &'static str) -> Self {
        Self::MissingField(field)
    }

    /// Constructs a soundness-failure error from a validator's message.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// The name of the field that was not set, or `None` for an
    /// [`Invalid`](BuilderError::Invalid) error.
    #[must_use]
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

    // Deliberately neither Clone nor Copy: the consuming default must only move.
    enum OwnedKind {
        Named,
        Other(u8),
    }

    impl NormalizeEnumAliases for OwnedKind {
        fn normalize_enum_aliases(&mut self) {
            if matches!(self, Self::Other(2)) {
                *self = Self::Named;
            }
        }
    }

    #[test]
    fn consuming_normalization_needs_neither_clone_nor_copy() {
        assert!(matches!(
            OwnedKind::Other(2).into_normalized_enum_aliases(),
            OwnedKind::Named
        ));
        assert!(matches!(
            OwnedKind::Other(99).into_normalized_enum_aliases(),
            OwnedKind::Other(99)
        ));
    }

    #[test]
    fn normalization_remains_dyn_compatible() {
        let mut value = OwnedKind::Other(2);
        let erased: &mut dyn NormalizeEnumAliases = &mut value;
        erased.normalize_enum_aliases();
        assert!(matches!(value, OwnedKind::Named));
    }

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
