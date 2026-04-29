// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

//! Human-readable RF unit parsing.
//!
//! The crate exposes small domain types for frequencies, sample rates, and
//! scan targets while preserving exact integer semantics. Inputs may use
//! underscores in numeric literals, optional whitespace, decimal fractions that
//! resolve to whole Hz/S/s values, and case-insensitive unit suffixes such as
//! `mhz`, `MHz`, `MS/s`, and `msps`.
//!
//! Frequency quantities accept no suffix, `h`, `hz`, `k`, `khz`, `m`, `mhz`,
//! `g`, or `ghz`. Sample-rate quantities accept the same suffixes plus `sps`
//! and `s/s` forms such as `ksps`, `MS/s`, and `gs/s`.
//!
//! # Examples
//!
//! ```
//! # use rfus::{FrequencyHz, FrequencyRange, SampleRateSps, ScanTarget};
//! let frequency: FrequencyHz = "450.5 MHz".parse().unwrap();
//! let rate: SampleRateSps = "2 MS/s".parse().unwrap();
//! let range: FrequencyRange = "400m-520m".parse().unwrap();
//! let target: ScanTarget = "400m-520m; 800m-900m".parse().unwrap();
//!
//! assert_eq!(frequency.hz(), 450_500_000);
//! assert_eq!(rate.sps(), 2_000_000);
//! assert_eq!(range.canonical(), "400000000,520000000");
//! assert!(matches!(target, ScanTarget::Ranges(_)));
//! ```

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use winnow::Parser;
use winnow::ascii::multispace0;
use winnow::combinator::{alt, eof, opt, preceded, separated, separated_pair, terminated};
use winnow::error::{ContextError, ErrMode};
use winnow::token::{one_of, take_while};

/// Lowest accepted RF frequency for user-facing frequency inputs.
pub const MIN_FREQUENCY_HZ: u64 = 1_000_000;
/// Highest accepted RF frequency for user-facing frequency inputs.
pub const MAX_FREQUENCY_HZ: u64 = u64::MAX;
/// Lowest accepted sample rate in samples per second.
pub const MIN_SAMPLE_RATE_SPS: u32 = 200_000;
/// Highest accepted sample rate in samples per second.
pub const MAX_SAMPLE_RATE_SPS: u32 = 20_000_000;

/// A generic hertz quantity.
///
/// Use this for bandwidths, offsets, and other Hz values that should not be
/// constrained by the RF frequency floor. Display formatting emits the bare
/// integer Hz value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hertz(u64);

impl Hertz {
    /// Return the quantity in Hz.
    pub const fn hz(self) -> u64 {
        self.0
    }

    /// Return the quantity as `u32` when required by hardware APIs.
    pub fn as_u32(self) -> Result<u32, ParseUnitError> {
        validate_range(self.0, 0, u32::MAX as u64, "Hz")?;
        Ok(self.0 as u32)
    }
}

impl Display for Hertz {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Hertz {
    type Err = ParseUnitError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse_unit_value(input, UnitSet::Frequency)?))
    }
}

/// A validated RF frequency in Hz.
///
/// Parsing enforces [`MIN_FREQUENCY_HZ`] and [`MAX_FREQUENCY_HZ`]. Use
/// [`FrequencyHz::new_unchecked`] only at trusted boundaries where validation
/// has already happened. Display formatting emits the bare integer Hz value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrequencyHz(u64);

impl FrequencyHz {
    /// Create a frequency without applying range validation.
    ///
    /// This constructor is intended for values that have already been checked
    /// by an external source such as a hardware inventory or protocol schema.
    pub const fn new_unchecked(value: u64) -> Self {
        Self(value)
    }

    /// Create a validated frequency.
    pub fn new(value: u64) -> Result<Self, ParseUnitError> {
        validate_range(value, MIN_FREQUENCY_HZ, MAX_FREQUENCY_HZ, "Hz")?;
        Ok(Self(value))
    }

    /// Return the frequency in Hz.
    pub const fn hz(self) -> u64 {
        self.0
    }
}

impl Display for FrequencyHz {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for FrequencyHz {
    type Err = ParseUnitError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let hz = parse_unit_value(input, UnitSet::Frequency)?;
        Self::new(hz)
    }
}

/// A sample rate in samples per second.
///
/// Sample rates accept both frequency-style units (`2MHz`) and sample-rate
/// units (`2MS/s`, `2msps`) because hardware and CLI users commonly use both.
/// Display formatting emits the bare integer S/s value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRateSps(u32);

impl SampleRateSps {
    /// Create a sample rate without applying range validation.
    ///
    /// This constructor is intended for values that have already been checked
    /// by an external source such as a hardware inventory or protocol schema.
    pub const fn new_unchecked(value: u32) -> Self {
        Self(value)
    }

    /// Create a validated sample rate.
    pub fn new(value: u32) -> Result<Self, ParseUnitError> {
        validate_range(
            value as u64,
            MIN_SAMPLE_RATE_SPS as u64,
            MAX_SAMPLE_RATE_SPS as u64,
            "S/s",
        )?;
        Ok(Self(value))
    }

    /// Return the sample rate in samples per second.
    pub const fn sps(self) -> u32 {
        self.0
    }
}

impl Display for SampleRateSps {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SampleRateSps {
    type Err = ParseUnitError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let value = parse_unit_value(input, UnitSet::SampleRate)?;
        validate_range(
            value,
            MIN_SAMPLE_RATE_SPS as u64,
            MAX_SAMPLE_RATE_SPS as u64,
            "S/s",
        )?;
        Ok(Self(value as u32))
    }
}

/// A frequency range with an exclusive ordering invariant: `lower < upper`.
///
/// Display formatting and [`FrequencyRange::canonical`] both emit
/// `lower,upper` as bare integer Hz values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrequencyRange {
    /// Lower bound in Hz.
    pub lower: FrequencyHz,
    /// Upper bound in Hz.
    pub upper: FrequencyHz,
}

impl FrequencyRange {
    /// Create a range and validate that `lower < upper`.
    pub fn new(lower: FrequencyHz, upper: FrequencyHz) -> Result<Self, ParseUnitError> {
        if lower >= upper {
            return Err(ParseUnitError::InvalidRange {
                lower: lower.hz(),
                upper: upper.hz(),
            });
        }
        Ok(Self { lower, upper })
    }

    /// Return the canonical wire/debug form: `lower,upper` in integer Hz.
    pub fn canonical(&self) -> String {
        format!("{},{}", self.lower.hz(), self.upper.hz())
    }
}

impl Display for FrequencyRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.lower.hz(), self.upper.hz())
    }
}

impl FromStr for FrequencyRange {
    type Err = ParseUnitError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_complete(input, frequency_range_tokens)?.resolve()
    }
}

/// A scan target provided by the user.
///
/// A single frequency such as `450m` becomes [`ScanTarget::Static`]. One or
/// more ranges such as `1mhz-20mhz; 400m-520m` become [`ScanTarget::Ranges`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanTarget {
    /// A single static frequency in Hz.
    Static(FrequencyHz),
    /// One or more frequency ranges.
    Ranges(Vec<FrequencyRange>),
}

impl FromStr for ScanTarget {
    type Err = ParseUnitError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_complete(input, scan_target_tokens)?.resolve()
    }
}

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

/// Parse a user-facing RF frequency and return Hz.
pub fn parse_frequency_hz(input: &str) -> Result<u64, ParseUnitError> {
    Ok(FrequencyHz::from_str(input)?.hz())
}

/// Parse a generic Hz quantity and return it as `u32`.
pub fn parse_hertz_u32(input: &str) -> Result<u32, ParseUnitError> {
    Hertz::from_str(input)?.as_u32()
}

/// Parse a user-facing sample rate and return samples per second.
pub fn parse_sample_rate_sps(input: &str) -> Result<u32, ParseUnitError> {
    Ok(SampleRateSps::from_str(input)?.sps())
}

/// Parse a single frequency range.
pub fn parse_frequency_range(input: &str) -> Result<FrequencyRange, ParseUnitError> {
    FrequencyRange::from_str(input)
}

/// Parse one or more frequency ranges.
pub fn parse_frequency_ranges(input: &str) -> Result<Vec<FrequencyRange>, ParseUnitError> {
    match ScanTarget::from_str(input)? {
        ScanTarget::Ranges(ranges) => Ok(ranges),
        ScanTarget::Static(freq) => Err(ParseUnitError::Parse(format!(
            "{} is a static frequency, not a range",
            freq.hz()
        ))),
    }
}

fn parse_complete<'a, T>(
    input: &'a str,
    parser: impl Parser<&'a str, T, ErrMode<ContextError>>,
) -> Result<T, ParseUnitError> {
    if input.trim().is_empty() {
        return Err(ParseUnitError::Empty);
    }

    terminated(parser, (multispace0, eof))
        .parse(input)
        .map_err(|_| ParseUnitError::Parse(input.to_string()))
}

fn parse_unit_value(input: &str, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
    parse_complete(input, scalar_token)?.resolve(unit_set)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarToken<'a> {
    number: &'a str,
    unit: &'a str,
}

impl ScalarToken<'_> {
    fn resolve(self, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
        let multiplier = multiplier_for(self.unit, unit_set)?;
        scale_number(self.number, multiplier, unit_set.base_unit())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrequencyRangeTokens<'a> {
    lower: ScalarToken<'a>,
    upper: ScalarToken<'a>,
}

impl FrequencyRangeTokens<'_> {
    fn resolve(self) -> Result<FrequencyRange, ParseUnitError> {
        let lower = FrequencyHz::new(self.lower.resolve(UnitSet::Frequency)?)?;
        let upper = FrequencyHz::new(self.upper.resolve(UnitSet::Frequency)?)?;
        FrequencyRange::new(lower, upper)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScanTargetTokens<'a> {
    Static(ScalarToken<'a>),
    Ranges(Vec<FrequencyRangeTokens<'a>>),
}

impl ScanTargetTokens<'_> {
    fn resolve(self) -> Result<ScanTarget, ParseUnitError> {
        match self {
            Self::Static(token) => {
                let hz = token.resolve(UnitSet::Frequency)?;
                Ok(ScanTarget::Static(FrequencyHz::new(hz)?))
            }
            Self::Ranges(ranges) => ranges
                .into_iter()
                .map(FrequencyRangeTokens::resolve)
                .collect::<Result<Vec<_>, _>>()
                .map(ScanTarget::Ranges),
        }
    }
}

fn scan_target_tokens<'a>(input: &mut &'a str) -> winnow::ModalResult<ScanTargetTokens<'a>> {
    alt((
        ranges_list_tokens.map(ScanTargetTokens::Ranges),
        scalar_token.map(ScanTargetTokens::Static),
    ))
    .parse_next(input)
}

fn ranges_list_tokens<'a>(
    input: &mut &'a str,
) -> winnow::ModalResult<Vec<FrequencyRangeTokens<'a>>> {
    terminated(
        separated(1.., frequency_range_tokens, range_list_separator),
        opt(range_list_separator),
    )
    .parse_next(input)
}

fn range_list_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, ';').parse_next(input)
}

fn frequency_range_tokens<'a>(
    input: &mut &'a str,
) -> winnow::ModalResult<FrequencyRangeTokens<'a>> {
    separated_pair(scalar_token, range_separator, scalar_token)
        .map(|(lower, upper)| FrequencyRangeTokens { lower, upper })
        .parse_next(input)
}

fn range_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, alt((',', '-'))).parse_next(input)
}

fn scalar_token<'a>(input: &mut &'a str) -> winnow::ModalResult<ScalarToken<'a>> {
    let number = preceded(multispace0, number_literal).parse_next(input)?;
    let unit = opt(unit_literal).parse_next(input)?.unwrap_or("");
    Ok(ScalarToken { number, unit })
}

fn number_literal<'a>(input: &mut &'a str) -> winnow::ModalResult<&'a str> {
    (
        take_while(1.., |c: char| c.is_ascii_digit() || c == '_'),
        opt((
            one_of('.'),
            take_while(1.., |c: char| c.is_ascii_digit() || c == '_'),
        )),
    )
        .take()
        .parse_next(input)
}

fn unit_literal<'a>(input: &mut &'a str) -> winnow::ModalResult<&'a str> {
    preceded(multispace0, take_while(1.., is_unit_char)).parse_next(input)
}

fn is_unit_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '/'
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnitSet {
    Frequency,
    SampleRate,
}

impl UnitSet {
    const fn base_unit(self) -> &'static str {
        match self {
            Self::Frequency => "Hz",
            Self::SampleRate => "S/s",
        }
    }
}

const FREQUENCY_UNITS: &[(&str, u64)] = &[
    ("", 1),
    ("h", 1),
    ("hz", 1),
    ("k", 1_000),
    ("khz", 1_000),
    ("m", 1_000_000),
    ("mhz", 1_000_000),
    ("g", 1_000_000_000),
    ("ghz", 1_000_000_000),
];

const SAMPLE_RATE_UNITS: &[(&str, u64)] = &[
    ("s/s", 1),
    ("sps", 1),
    ("ks/s", 1_000),
    ("ksps", 1_000),
    ("ms/s", 1_000_000),
    ("msps", 1_000_000),
    ("gs/s", 1_000_000_000),
    ("gsps", 1_000_000_000),
];

fn multiplier_for(unit: &str, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
    let normalized = unit.to_ascii_lowercase();

    if let Some(multiplier) = find_multiplier(&normalized, FREQUENCY_UNITS) {
        return Ok(multiplier);
    }
    match unit_set {
        UnitSet::Frequency => {}
        UnitSet::SampleRate => {
            if let Some(multiplier) = find_multiplier(&normalized, SAMPLE_RATE_UNITS) {
                return Ok(multiplier);
            }
        }
    }

    Err(ParseUnitError::UnknownUnit(normalized))
}

fn find_multiplier(unit: &str, units: &[(&str, u64)]) -> Option<u64> {
    units
        .iter()
        .find_map(|(name, multiplier)| (*name == unit).then_some(*multiplier))
}

fn scale_number(
    number: &str,
    multiplier: u64,
    base_unit: &'static str,
) -> Result<u64, ParseUnitError> {
    let (whole, fractional) = number.split_once('.').unwrap_or((number, ""));
    let whole = normalize_digits(whole, number)?;
    let fractional = normalize_digits(fractional, number)?;

    if whole.is_empty() && fractional.is_empty() {
        return Err(ParseUnitError::InvalidNumber(number.to_string()));
    }

    let whole_value = parse_u128_digits(&whole, number)?;
    let fractional_value = parse_u128_digits(&fractional, number)?;
    let scale = 10_u128
        .checked_pow(fractional.len() as u32)
        .ok_or_else(|| overflow_error(base_unit))?;

    let numerator = whole_value
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fractional_value))
        .and_then(|value| value.checked_mul(multiplier as u128))
        .ok_or_else(|| overflow_error(base_unit))?;

    if numerator % scale != 0 {
        return Err(ParseUnitError::NonInteger(number.to_string()));
    }

    let value = numerator / scale;
    if value > u64::MAX as u128 {
        return Err(overflow_error(base_unit));
    }
    Ok(value as u64)
}

fn overflow_error(unit: &'static str) -> ParseUnitError {
    ParseUnitError::OutOfRange {
        value: u64::MAX,
        min: 0,
        max: u64::MAX,
        unit,
    }
}

fn normalize_digits(input: &str, original: &str) -> Result<String, ParseUnitError> {
    if input.is_empty() {
        return Ok(String::new());
    }
    if input.starts_with('_') || input.ends_with('_') || input.contains("__") {
        return Err(ParseUnitError::InvalidNumber(original.to_string()));
    }

    let normalized = input.replace('_', "");
    if !normalized.is_empty() && normalized.chars().all(|c| c.is_ascii_digit()) {
        Ok(normalized)
    } else {
        Err(ParseUnitError::InvalidNumber(original.to_string()))
    }
}

fn parse_u128_digits(input: &str, original: &str) -> Result<u128, ParseUnitError> {
    if input.is_empty() {
        return Ok(0);
    }
    input
        .parse::<u128>()
        .map_err(|_| ParseUnitError::InvalidNumber(original.to_string()))
}

fn validate_range(
    value: u64,
    min: u64,
    max: u64,
    unit: &'static str,
) -> Result<(), ParseUnitError> {
    if !(min..=max).contains(&value) {
        return Err(ParseUnitError::OutOfRange {
            value,
            min,
            max,
            unit,
        });
    }
    Ok(())
}

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{FrequencyHz, FrequencyRange, SampleRateSps, ScanTarget};
    use serde::de::{self, Visitor};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::fmt;
    use std::marker::PhantomData;
    use std::str::FromStr;

    impl Serialize for FrequencyHz {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_u64(self.hz())
        }
    }

    impl<'de> Deserialize<'de> for FrequencyHz {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(NumberOrStringVisitor::<FrequencyHz>::new())
        }
    }

    impl Serialize for FrequencyRange {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            #[derive(Serialize)]
            struct Range {
                lower: u64,
                upper: u64,
            }

            Range {
                lower: self.lower.hz(),
                upper: self.upper.hz(),
            }
            .serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for FrequencyRange {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Repr {
                String(String),
                Object {
                    lower: FrequencyHz,
                    upper: FrequencyHz,
                },
            }

            match Repr::deserialize(deserializer)? {
                Repr::String(value) => FrequencyRange::from_str(&value).map_err(de::Error::custom),
                Repr::Object { lower, upper } => {
                    FrequencyRange::new(lower, upper).map_err(de::Error::custom)
                }
            }
        }
    }

    impl Serialize for SampleRateSps {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_u32(self.sps())
        }
    }

    impl<'de> Deserialize<'de> for SampleRateSps {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(NumberOrStringVisitor::<SampleRateSps>::new())
        }
    }

    trait NumberOrStringUnit: Sized {
        const EXPECTING: &'static str;

        fn from_number<E>(value: u64) -> Result<Self, E>
        where
            E: de::Error;

        fn from_string<E>(value: &str) -> Result<Self, E>
        where
            E: de::Error;
    }

    struct NumberOrStringVisitor<T>(PhantomData<T>);

    impl<T> NumberOrStringVisitor<T> {
        const fn new() -> Self {
            Self(PhantomData)
        }
    }

    impl<'de, T> Visitor<'de> for NumberOrStringVisitor<T>
    where
        T: NumberOrStringUnit,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(T::EXPECTING)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            T::from_number(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            T::from_string(value)
        }
    }

    impl NumberOrStringUnit for FrequencyHz {
        const EXPECTING: &'static str = "a frequency as Hz number or human-readable string";

        fn from_number<E>(value: u64) -> Result<Self, E>
        where
            E: de::Error,
        {
            FrequencyHz::new(value).map_err(E::custom)
        }

        fn from_string<E>(value: &str) -> Result<Self, E>
        where
            E: de::Error,
        {
            FrequencyHz::from_str(value).map_err(E::custom)
        }
    }

    impl NumberOrStringUnit for SampleRateSps {
        const EXPECTING: &'static str = "a sample rate as S/s number or human-readable string";

        fn from_number<E>(value: u64) -> Result<Self, E>
        where
            E: de::Error,
        {
            let value = u32::try_from(value).map_err(E::custom)?;
            SampleRateSps::new(value).map_err(E::custom)
        }

        fn from_string<E>(value: &str) -> Result<Self, E>
        where
            E: de::Error,
        {
            SampleRateSps::from_str(value).map_err(E::custom)
        }
    }

    impl Serialize for ScanTarget {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match self {
                ScanTarget::Static(freq) => {
                    #[derive(Serialize)]
                    struct Static {
                        static_frequency: u64,
                    }
                    Static {
                        static_frequency: freq.hz(),
                    }
                    .serialize(serializer)
                }
                ScanTarget::Ranges(ranges) => {
                    #[derive(Serialize)]
                    struct Ranges<'a> {
                        ranges: &'a [FrequencyRange],
                    }
                    Ranges { ranges }.serialize(serializer)
                }
            }
        }
    }

    impl<'de> Deserialize<'de> for ScanTarget {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Repr {
                String(String),
                Static { static_frequency: FrequencyHz },
                Ranges { ranges: Vec<FrequencyRange> },
            }

            match Repr::deserialize(deserializer)? {
                Repr::String(value) => ScanTarget::from_str(&value).map_err(de::Error::custom),
                Repr::Static { static_frequency } => Ok(ScanTarget::Static(static_frequency)),
                Repr::Ranges { ranges } => Ok(ScanTarget::Ranges(ranges)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frequency_suffixes_case_insensitively() {
        assert_eq!("1mhz".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
        assert_eq!("1 MHz".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
        assert_eq!("1M".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
        assert_eq!("1.5MHz".parse::<FrequencyHz>().unwrap().hz(), 1_500_000);
        assert_eq!("450.5m".parse::<FrequencyHz>().unwrap().hz(), 450_500_000);
        assert_eq!("1_000_000".parse::<FrequencyHz>().unwrap().hz(), 1_000_000);
        assert_eq!("7g".parse::<FrequencyHz>().unwrap().hz(), 7_000_000_000);
        assert!("2msps".parse::<FrequencyHz>().is_err());
    }

    #[test]
    fn parses_sample_rate_suffixes() {
        assert_eq!("2msps".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
        assert_eq!("2 MS/s".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
        assert_eq!("2mhz".parse::<SampleRateSps>().unwrap().sps(), 2_000_000);
        assert_eq!("500ksps".parse::<SampleRateSps>().unwrap().sps(), 500_000);
        assert_eq!(
            "1_000_000".parse::<SampleRateSps>().unwrap().sps(),
            1_000_000
        );
    }

    #[test]
    fn parses_unbounded_hertz_for_bandwidths() {
        assert_eq!("12.5khz".parse::<Hertz>().unwrap().hz(), 12_500);
        assert_eq!(parse_hertz_u32("12500").unwrap(), 12_500);
    }

    #[test]
    fn rejects_sample_rate_bounds() {
        assert!("100ksps".parse::<SampleRateSps>().is_err());
        assert!("25msps".parse::<SampleRateSps>().is_err());
    }

    #[test]
    fn parses_frequency_ranges() {
        let range = "1mhz-20mhz".parse::<FrequencyRange>().unwrap();
        assert_eq!(range.lower.hz(), 1_000_000);
        assert_eq!(range.upper.hz(), 20_000_000);
        assert_eq!(
            "1m,2m".parse::<FrequencyRange>().unwrap().canonical(),
            "1000000,2000000"
        );
        assert_eq!(
            "400_000_000, 520_000_000"
                .parse::<FrequencyRange>()
                .unwrap()
                .canonical(),
            "400000000,520000000"
        );
    }

    #[test]
    fn parses_scan_targets() {
        assert!(matches!(
            "450m".parse::<ScanTarget>().unwrap(),
            ScanTarget::Static(_)
        ));

        match "1m-2m; 400m-520m".parse::<ScanTarget>().unwrap() {
            ScanTarget::Ranges(ranges) => {
                assert_eq!(ranges.len(), 2);
                assert_eq!(ranges[0].lower.hz(), 1_000_000);
                assert_eq!(ranges[1].upper.hz(), 520_000_000);
            }
            ScanTarget::Static(_) => panic!("expected ranges"),
        }
    }

    #[test]
    fn rejects_invalid_ranges() {
        assert_eq!(
            "2m-1m".parse::<FrequencyRange>().unwrap_err(),
            ParseUnitError::InvalidRange {
                lower: 2_000_000,
                upper: 1_000_000,
            }
        );
        assert!("1m".parse::<FrequencyRange>().is_err());
        assert_eq!(
            "1m-1m".parse::<FrequencyRange>().unwrap_err(),
            ParseUnitError::InvalidRange {
                lower: 1_000_000,
                upper: 1_000_000,
            }
        );
    }

    #[test]
    fn reports_specific_scalar_errors() {
        assert_eq!(
            "2msps".parse::<FrequencyHz>().unwrap_err(),
            ParseUnitError::UnknownUnit("msps".to_string())
        );
        assert_eq!(
            "2widgets".parse::<FrequencyHz>().unwrap_err(),
            ParseUnitError::UnknownUnit("widgets".to_string())
        );
        assert_eq!(
            "1.0000001hz".parse::<Hertz>().unwrap_err(),
            ParseUnitError::NonInteger("1.0000001".to_string())
        );
        assert_eq!(
            "_mhz".parse::<Hertz>().unwrap_err(),
            ParseUnitError::InvalidNumber("_".to_string())
        );
    }

    #[test]
    fn requires_semicolons_between_ranges() {
        assert!("1m-2m 3m-4m".parse::<ScanTarget>().is_err());

        match "1m-2m;".parse::<ScanTarget>().unwrap() {
            ScanTarget::Ranges(ranges) => assert_eq!(ranges.len(), 1),
            ScanTarget::Static(_) => panic!("expected ranges"),
        }
    }

    #[test]
    fn accepts_whitespace_around_numbers_units_and_separators() {
        assert_eq!(
            "\t2 MS/s\n".parse::<SampleRateSps>().unwrap().sps(),
            2_000_000
        );
        assert_eq!(
            " 1 MHz , 2 MHz "
                .parse::<FrequencyRange>()
                .unwrap()
                .canonical(),
            "1000000,2000000"
        );
    }

    #[test]
    fn validates_underscore_and_decimal_literals() {
        assert_eq!("1_000.5khz".parse::<Hertz>().unwrap().hz(), 1_000_500);
        assert_eq!("0.2MS/s".parse::<SampleRateSps>().unwrap().sps(), 200_000);
        assert_eq!(
            "1__000mhz".parse::<Hertz>().unwrap_err(),
            ParseUnitError::InvalidNumber("1__000".to_string())
        );
        assert_eq!(
            "1_mhz".parse::<Hertz>().unwrap_err(),
            ParseUnitError::InvalidNumber("1_".to_string())
        );
    }

    #[test]
    fn reports_overflow_and_u32_bounds() {
        assert_eq!(
            parse_hertz_u32("4294967296hz").unwrap_err(),
            ParseUnitError::OutOfRange {
                value: 4_294_967_296,
                min: 0,
                max: u32::MAX as u64,
                unit: "Hz",
            }
        );
        assert_eq!(
            "18446744073709551616hz".parse::<Hertz>().unwrap_err(),
            ParseUnitError::OutOfRange {
                value: u64::MAX,
                min: 0,
                max: u64::MAX,
                unit: "Hz",
            }
        );
        assert_eq!(
            "4294967296".parse::<SampleRateSps>().unwrap_err(),
            ParseUnitError::OutOfRange {
                value: 4_294_967_296,
                min: MIN_SAMPLE_RATE_SPS as u64,
                max: MAX_SAMPLE_RATE_SPS as u64,
                unit: "S/s",
            }
        );
    }

    #[test]
    fn public_parse_helpers_match_from_str() {
        assert_eq!(parse_frequency_hz("450m").unwrap(), 450_000_000);
        assert_eq!(parse_sample_rate_sps("2msps").unwrap(), 2_000_000);
        assert_eq!(
            parse_frequency_range("1m-2m").unwrap().canonical(),
            "1000000,2000000"
        );
        assert_eq!(parse_frequency_ranges("1m-2m;3m-4m").unwrap().len(), 2);
        assert!(parse_frequency_ranges("450m").is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_accepts_dense_and_explicit_scan_targets() {
        assert!(matches!(
            serde_json::from_str::<ScanTarget>(r#""450m""#).unwrap(),
            ScanTarget::Static(_)
        ));
        match serde_json::from_str::<ScanTarget>(
            r#"{ "ranges": [{ "lower": "400m", "upper": "520m" }] }"#,
        )
        .unwrap()
        {
            ScanTarget::Ranges(ranges) => {
                assert_eq!(ranges[0].lower.hz(), 400_000_000);
                assert_eq!(ranges[0].upper.hz(), 520_000_000);
            }
            ScanTarget::Static(_) => panic!("expected ranges"),
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_checks_numeric_boundaries() {
        assert!(serde_json::from_str::<FrequencyHz>("999999").is_err());
        assert_eq!(
            serde_json::from_str::<SampleRateSps>("200000")
                .unwrap()
                .sps(),
            MIN_SAMPLE_RATE_SPS
        );
        assert!(serde_json::from_str::<SampleRateSps>("20000001").is_err());
        assert!(
            serde_json::from_str::<FrequencyRange>(r#"{ "lower": 2000000, "upper": 1000000 }"#)
                .is_err()
        );
    }
}
