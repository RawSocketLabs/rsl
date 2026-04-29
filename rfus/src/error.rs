// RawSocket Labs LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Error returned when a unit-bearing value cannot be parsed or validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseUnitError {
    /// The input was empty or whitespace-only.
    Empty,
    /// The numeric literal was malformed.
    InvalidNumber(String),
    /// The suffix is not valid for the requested unit family.
    UnknownUnit(String),
    /// The scaled value did not resolve to a whole Hz/S/s integer.
    NonInteger(String),
    /// The parsed value is outside the accepted range.
    OutOfRange {
        value: u64,
        min: u64,
        max: u64,
        unit: &'static str,
    },
    /// A range was provided with `lower >= upper`.
    InvalidRange { lower: u64, upper: u64 },
    /// The input did not match the expected grammar.
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
