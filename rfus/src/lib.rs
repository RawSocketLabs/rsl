// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

//! Human-readable RF unit parsing.
//!
//! The crate exposes small domain types for frequencies, sample rates, and
//! scan targets while preserving exact integer semantics. Inputs may use
//! underscores in numeric literals, optional whitespace, decimal fractions that
//! resolve to whole Hz/S/s values, and case-insensitive unit suffixes such as
//! `mhz`, `MHz`, `MS/s`, and `msps`.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use winnow::Parser;
use winnow::ascii::{Caseless, multispace0};
use winnow::combinator::{alt, eof, opt, preceded, repeat, separated_pair, terminated};
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
/// constrained by the RF frequency floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hertz(u64);

impl Hertz {
    /// Return the value in Hz.
    pub const fn hz(self) -> u64 {
        self.0
    }

    /// Return the value as `u32` when required by hardware APIs.
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
        Ok(Self(parse_complete(input, frequency_value)?))
    }
}

/// An RF frequency in Hz.
///
/// Parsing enforces [`MIN_FREQUENCY_HZ`] and [`MAX_FREQUENCY_HZ`]. Use
/// [`FrequencyHz::new_unchecked`] only at trusted boundaries where validation
/// has already happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrequencyHz(u64);

impl FrequencyHz {
    /// Create a frequency without applying range validation.
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
        let hz = parse_complete(input, frequency_value)?;
        Self::new(hz)
    }
}

/// A sample rate in samples per second.
///
/// Sample rates accept both frequency-style units (`2MHz`) and sample-rate
/// units (`2MS/s`, `2msps`) because hardware and CLI users commonly use both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRateSps(u32);

impl SampleRateSps {
    /// Create a sample rate without applying range validation.
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
        let value = parse_complete(input, sample_rate_value)?;
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

    /// Return the canonical wire/debug form: `lower,upper` in Hz.
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
        parse_complete(input, frequency_range)
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
        parse_complete(input, scan_target)
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
            Self::NonInteger(value) => write!(f, "'{}' does not resolve to whole Hz", value),
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

fn scan_target(input: &mut &str) -> winnow::ModalResult<ScanTarget> {
    alt((
        ranges_list.map(ScanTarget::Ranges),
        frequency.map(ScanTarget::Static),
    ))
    .parse_next(input)
}

fn ranges_list(input: &mut &str) -> winnow::ModalResult<Vec<FrequencyRange>> {
    repeat(1.., terminated(frequency_range, opt(range_list_separator))).parse_next(input)
}

fn range_list_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, ';').parse_next(input)
}

fn frequency_range(input: &mut &str) -> winnow::ModalResult<FrequencyRange> {
    let (lower, upper) = separated_pair(frequency, range_separator, frequency).parse_next(input)?;
    FrequencyRange::new(lower, upper).map_err(|_| ErrMode::Cut(ContextError::new()))
}

fn range_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, alt((',', '-'))).parse_next(input)
}

fn frequency(input: &mut &str) -> winnow::ModalResult<FrequencyHz> {
    let hz = frequency_value.parse_next(input)?;
    FrequencyHz::new(hz).map_err(|_| ErrMode::Cut(ContextError::new()))
}

fn frequency_value(input: &mut &str) -> winnow::ModalResult<u64> {
    scaled_value(UnitSet::Frequency).parse_next(input)
}

fn sample_rate_value(input: &mut &str) -> winnow::ModalResult<u64> {
    scaled_value(UnitSet::SampleRate).parse_next(input)
}

fn scaled_value(unit_set: UnitSet) -> impl FnMut(&mut &str) -> winnow::ModalResult<u64> {
    move |input| {
        let number = preceded(multispace0, number_literal).parse_next(input)?;
        let unit = opt(unit_literal).parse_next(input)?.unwrap_or("");
        let multiplier =
            multiplier_for(unit, unit_set).map_err(|_| ErrMode::Cut(ContextError::new()))?;
        scale_number(number, multiplier).map_err(|_| ErrMode::Cut(ContextError::new()))
    }
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
    preceded(
        multispace0,
        alt((
            Caseless("s/s"),
            Caseless("ks/s"),
            Caseless("ms/s"),
            Caseless("gs/s"),
            Caseless("sps"),
            Caseless("ksps"),
            Caseless("msps"),
            Caseless("gsps"),
            Caseless("hz"),
            Caseless("khz"),
            Caseless("mhz"),
            Caseless("ghz"),
            Caseless("h"),
            Caseless("k"),
            Caseless("m"),
            Caseless("g"),
        )),
    )
    .parse_next(input)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnitSet {
    Frequency,
    SampleRate,
}

fn multiplier_for(unit: &str, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
    let normalized = unit.to_ascii_lowercase();
    let multiplier = match normalized.as_str() {
        "" | "hz" | "h" => 1,
        "k" | "khz" => 1_000,
        "m" | "mhz" => 1_000_000,
        "g" | "ghz" => 1_000_000_000,
        "sps" | "s/s" if unit_set == UnitSet::SampleRate => 1,
        "ksps" | "ks/s" if unit_set == UnitSet::SampleRate => 1_000,
        "msps" | "ms/s" if unit_set == UnitSet::SampleRate => 1_000_000,
        "gsps" | "gs/s" if unit_set == UnitSet::SampleRate => 1_000_000_000,
        other => return Err(ParseUnitError::UnknownUnit(other.to_string())),
    };
    Ok(multiplier)
}

fn scale_number(number: &str, multiplier: u64) -> Result<u64, ParseUnitError> {
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
        .ok_or(ParseUnitError::OutOfRange {
            value: u64::MAX,
            min: 0,
            max: u64::MAX,
            unit: "Hz",
        })?;

    let numerator = whole_value
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fractional_value))
        .and_then(|value| value.checked_mul(multiplier as u128))
        .ok_or(ParseUnitError::OutOfRange {
            value: u64::MAX,
            min: 0,
            max: u64::MAX,
            unit: "Hz",
        })?;

    if numerator % scale != 0 {
        return Err(ParseUnitError::NonInteger(number.to_string()));
    }

    let value = numerator / scale;
    if value > u64::MAX as u128 {
        return Err(ParseUnitError::OutOfRange {
            value: u64::MAX,
            min: 0,
            max: u64::MAX,
            unit: "Hz",
        });
    }
    Ok(value as u64)
}

fn normalize_digits(input: &str, original: &str) -> Result<String, ParseUnitError> {
    let normalized = input.replace('_', "");
    if normalized.chars().all(|c| c.is_ascii_digit()) {
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
            deserializer.deserialize_any(FrequencyVisitor)
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

    struct FrequencyVisitor;

    impl Visitor<'_> for FrequencyVisitor {
        type Value = FrequencyHz;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a frequency as Hz number or human-readable string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            FrequencyHz::new(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            FrequencyHz::from_str(value).map_err(E::custom)
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
            deserializer.deserialize_any(SampleRateVisitor)
        }
    }

    struct SampleRateVisitor;

    impl Visitor<'_> for SampleRateVisitor {
        type Value = SampleRateSps;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a sample rate as S/s number or human-readable string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let value = u32::try_from(value).map_err(E::custom)?;
            SampleRateSps::new(value).map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
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
        assert!("2m-1m".parse::<FrequencyRange>().is_err());
        assert!("1m".parse::<FrequencyRange>().is_err());
        assert!("1m-1m".parse::<FrequencyRange>().is_err());
        assert!("1.0000001hz".parse::<Hertz>().is_err());
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
}
