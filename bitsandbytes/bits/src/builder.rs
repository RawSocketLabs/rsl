//! Support types for `#[derive(BitsBuilder)]`.

use core::fmt;

/// The error a generated builder's `build()` returns when a required field was
/// never set.
///
/// "Required" is the default — a field is only optional if it carries
/// `#[builder(default)]` (or `#[builder(default = expr)]`). This is what lets the
/// builder *call out* an unset bit/byte instead of silently defaulting it to
/// zero (the gap in the infix `with_*` API).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderError {
    field: &'static str,
}

impl BuilderError {
    /// Constructs the "required field not set" error for `field`.
    pub fn missing_field(field: &'static str) -> Self {
        Self { field }
    }

    /// The name of the field that was not set.
    pub fn field(&self) -> &'static str {
        self.field
    }
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "required field `{}` was not set", self.field)
    }
}

impl std::error::Error for BuilderError {}
