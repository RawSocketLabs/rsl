use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Error returned when a unit-bearing value cannot be parsed or validated.
///
/// The parser distinguishes syntax failures from domain validation failures
/// when the input reaches a well-defined validation step. For example, an
/// unsupported suffix returns [`ParseUnitError::UnknownUnit`], while a range
/// with `lower >= upper` returns [`ParseUnitError::InvalidRange`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseUnitError {
    /// The input was empty or whitespace-only.
    Empty,
    /// The numeric literal was malformed.
    ///
    /// This covers misplaced underscores and integer literals that do not fit
    /// in the intermediate integer representation.
    InvalidNumber(String),
    /// The suffix is not valid for the requested unit family.
    ///
    /// Frequency parsing rejects sample-rate-only suffixes such as `msps`.
    UnknownUnit(String),
    /// The scaled value did not resolve to a whole Hz/S/s integer.
    ///
    /// For example, `1.0000001hz` is syntactically valid but cannot be
    /// represented as a whole hertz value.
    NonInteger(String),
    /// The parsed value is outside the accepted range.
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
    InvalidRange {
        /// Lower range endpoint in Hz.
        lower: u64,
        /// Upper range endpoint in Hz.
        upper: u64,
    },
    /// The input did not match the expected grammar.
    ///
    /// This is used when the parser cannot identify a more specific semantic
    /// error.
    Parse(String),
}

impl Display for ParseUnitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "value is empty"),
            Self::InvalidNumber(value) => write!(f, "'{}' is not a valid number", value),
            Self::UnknownUnit(unit) => write!(f, "'{}' is not a supported unit", unit),
            Self::NonInteger(value) => {
                write!(f, "'{}' does not resolve to a whole Hz or S/s value", value)
            }
            Self::OutOfRange {
                value,
                min,
                max,
                unit,
            } => write!(
                f,
                "{} {} is outside the valid range {}-{}",
                value, unit, min, max
            ),
            Self::InvalidRange { lower, upper } => {
                write!(f, "range lower {} must be less than upper {}", lower, upper)
            }
            Self::Parse(value) => write!(f, "could not parse '{}'", value),
        }
    }
}

impl Error for ParseUnitError {}
