use thiserror::Error;

/// Error returned when a unit-bearing value cannot be parsed or validated.
///
/// The parser distinguishes syntax failures from domain validation failures
/// when the input reaches a well-defined validation step. For example, an
/// unsupported suffix returns [`ParseUnitError::UnknownUnit`], while a range
/// with `lower >= upper` returns [`ParseUnitError::InvalidRange`].
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ParseUnitError {
    /// The input was empty or whitespace-only.
    #[error("value is empty")]
    Empty,
    /// The numeric literal was malformed.
    ///
    /// This covers misplaced underscores and integer literals that do not fit
    /// in the intermediate integer representation.
    #[error("'{0}' is not a valid number")]
    InvalidNumber(String),
    /// The suffix is not valid for the requested unit family.
    ///
    /// Frequency parsing rejects sample-rate-only suffixes such as `msps`.
    #[error("'{0}' is not a supported unit")]
    UnknownUnit(String),
    /// The scaled value did not resolve to a whole Hz/S/s integer.
    ///
    /// For example, `1.0000001hz` is syntactically valid but cannot be
    /// represented as a whole hertz value.
    #[error("'{0}' does not resolve to a whole Hz or S/s value")]
    NonInteger(String),
    /// The numeric literal, scaled by its suffix, does not fit `u64`.
    #[error("'{value}' scaled to {unit} does not fit u64")]
    Overflow {
        /// Numeric literal as written, without its unit suffix.
        value: String,
        /// Base unit the literal was scaled to, such as `Hz` or `S/s`.
        unit: &'static str,
    },
    /// The parsed value is outside the accepted range.
    #[error("{value} {unit} is outside the valid range {min}-{max}")]
    OutOfRange {
        /// Parsed value that failed validation.
        value: u64,
        /// Inclusive lower bound for the target unit family.
        min: u64,
        /// Inclusive upper bound for the target unit family.
        max: u64,
        /// Unit label used in diagnostics, such as `Hz` or `S/s`.
        unit: &'static str,
    },
    /// A range was provided with `lower >= upper`.
    #[error("range lower {lower} must be less than upper {upper}")]
    InvalidRange {
        /// Lower range endpoint in Hz.
        lower: u64,
        /// Upper range endpoint in Hz.
        upper: u64,
    },
    /// Range-only input was a single static frequency, in Hz.
    #[error("{0} Hz is a static frequency, not a range")]
    NotARange(u64),
    /// The input did not match the expected grammar.
    ///
    /// This is used when the parser cannot identify a more specific semantic
    /// error.
    #[error("could not parse '{0}'")]
    Parse(String),
}
