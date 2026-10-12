use std::str::FromStr;

use winnow::Parser;
use winnow::ascii::multispace0;
use winnow::combinator::{alt, eof, opt, preceded, separated, separated_pair, terminated};
use winnow::error::{ContextError, ErrMode};
use winnow::token::{one_of, take_while};

use crate::error::ParseUnitError;
use crate::types::{FrequencyHz, FrequencyRange, Hertz, SampleRateSps, ScanTarget};

/// Parse a user-facing RF frequency and return Hz.
///
/// This is a convenience wrapper around [`FrequencyHz`] parsing. Inputs must
/// resolve to a whole Hz integer.
///
/// # Examples
///
/// ```
/// assert_eq!(rfus::parse_frequency_hz("450m").unwrap(), 450_000_000);
/// ```
///
/// # Errors
///
/// Returns [`ParseUnitError`] when the input is not a whole-Hz frequency.
pub fn parse_frequency_hz(input: &str) -> Result<u64, ParseUnitError> {
    Ok(FrequencyHz::from_str(input)?.hz())
}

/// Parse a generic Hz quantity and return it as `u32`.
///
/// Use this for hardware APIs that accept Hz quantities as `u32`. Only the
/// `u32` conversion is checked; no RF range bounds are applied.
///
/// # Examples
///
/// ```
/// assert_eq!(rfus::parse_hertz_u32("12.5khz").unwrap(), 12_500);
/// ```
///
/// # Errors
///
/// Returns [`ParseUnitError`] when the input is not a whole-Hz quantity or
/// exceeds `u32::MAX`.
pub fn parse_hertz_u32(input: &str) -> Result<u32, ParseUnitError> {
    Hertz::from_str(input)?.as_u32()
}

/// Parse a user-facing sample rate and return samples per second.
///
/// Inputs may use frequency-style suffixes such as `mhz` or sample-rate
/// suffixes such as `MS/s` and `msps`. The result must fit `u32`.
///
/// # Examples
///
/// ```
/// assert_eq!(rfus::parse_sample_rate_sps("2msps").unwrap(), 2_000_000);
/// ```
///
/// # Errors
///
/// Returns [`ParseUnitError`] when the input is not a whole-S/s rate or
/// exceeds `u32::MAX`.
pub fn parse_sample_rate_sps(input: &str) -> Result<u32, ParseUnitError> {
    Ok(SampleRateSps::from_str(input)?.sps())
}

/// Parse a single frequency range.
///
/// Ranges accept `lower-upper` or `lower,upper`; both endpoints are parsed as
/// [`FrequencyHz`] and the final range must satisfy `lower < upper`.
///
/// # Examples
///
/// ```
/// let range = rfus::parse_frequency_range("400m-520m").unwrap();
/// assert_eq!(range.canonical(), "400000000,520000000");
/// ```
///
/// # Errors
///
/// Returns [`ParseUnitError`] when an endpoint fails to parse or
/// `lower >= upper`.
pub fn parse_frequency_range(input: &str) -> Result<FrequencyRange, ParseUnitError> {
    parse_complete(input, frequency_range_tokens)?.resolve()
}

/// Parse one or more frequency ranges.
///
/// Range lists are separated with semicolons. A single static frequency is
/// rejected because this helper is specifically for range-only input.
///
/// # Examples
///
/// ```
/// let ranges = rfus::parse_frequency_ranges("400m-520m;800m-900m").unwrap();
/// assert_eq!(ranges.len(), 2);
/// ```
///
/// # Errors
///
/// Returns [`ParseUnitError`] when any range fails to parse or the input is a
/// single static frequency.
pub fn parse_frequency_ranges(input: &str) -> Result<Vec<FrequencyRange>, ParseUnitError> {
    match parse_scan_target(input)? {
        ScanTarget::Ranges(ranges) => Ok(ranges),
        ScanTarget::Static(freq) => Err(ParseUnitError::Parse(format!(
            "{} is a static frequency, not a range",
            freq.hz()
        ))),
    }
}

/// Parse a static frequency or a semicolon-separated range list; backs `ScanTarget::from_str`.
pub(crate) fn parse_scan_target(input: &str) -> Result<ScanTarget, ParseUnitError> {
    parse_complete(input, scan_target_tokens)?.resolve()
}

/// Run `parser` over the whole input, allowing trailing whitespace, and map any
/// grammar failure to [`ParseUnitError::Parse`].
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

/// Parse one scalar quantity and scale it to the base unit of `unit_set`.
pub(crate) fn parse_unit_value(input: &str, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
    parse_complete(input, scalar_token)?.resolve(unit_set)
}

/// Unscaled scalar as written: the numeric literal and its (possibly empty) unit suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarToken<'a> {
    number: &'a str,
    unit: &'a str,
}

impl ScalarToken<'_> {
    /// Scale the literal by the multiplier of its suffix within `unit_set`.
    fn resolve(self, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
        let multiplier = multiplier_for(self.unit, unit_set)?;
        scale_number(self.number, multiplier, unit_set.base_unit())
    }
}

/// Unresolved endpoints of one `lower-upper` or `lower,upper` range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrequencyRangeTokens<'a> {
    lower: ScalarToken<'a>,
    upper: ScalarToken<'a>,
}

impl FrequencyRangeTokens<'_> {
    /// Resolve both endpoints as frequencies and enforce `lower < upper`.
    fn resolve(self) -> Result<FrequencyRange, ParseUnitError> {
        let lower = FrequencyHz::new(self.lower.resolve(UnitSet::Frequency)?);
        let upper = FrequencyHz::new(self.upper.resolve(UnitSet::Frequency)?);
        FrequencyRange::new(lower, upper)
    }
}

/// Unresolved scan target: one scalar or one or more ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScanTargetTokens<'a> {
    Static(ScalarToken<'a>),
    Ranges(Vec<FrequencyRangeTokens<'a>>),
}

impl ScanTargetTokens<'_> {
    /// Resolve every token as a frequency into a [`ScanTarget`].
    fn resolve(self) -> Result<ScanTarget, ParseUnitError> {
        match self {
            Self::Static(token) => {
                let hz = token.resolve(UnitSet::Frequency)?;
                Ok(ScanTarget::Static(FrequencyHz::new(hz)))
            }
            Self::Ranges(ranges) => ranges
                .into_iter()
                .map(FrequencyRangeTokens::resolve)
                .collect::<Result<Vec<_>, _>>()
                .map(ScanTarget::Ranges),
        }
    }
}

/// Grammar: a range list, falling back to a single scalar.
fn scan_target_tokens<'a>(input: &mut &'a str) -> winnow::ModalResult<ScanTargetTokens<'a>> {
    alt((
        ranges_list_tokens.map(ScanTargetTokens::Ranges),
        scalar_token.map(ScanTargetTokens::Static),
    ))
    .parse_next(input)
}

/// Grammar: one or more ranges separated by `;`, with an optional trailing `;`.
fn ranges_list_tokens<'a>(
    input: &mut &'a str,
) -> winnow::ModalResult<Vec<FrequencyRangeTokens<'a>>> {
    terminated(
        separated(1.., frequency_range_tokens, range_list_separator),
        opt(range_list_separator),
    )
    .parse_next(input)
}

/// Grammar: `;` with optional leading whitespace.
fn range_list_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, ';').parse_next(input)
}

/// Grammar: two scalars joined by a range separator.
fn frequency_range_tokens<'a>(
    input: &mut &'a str,
) -> winnow::ModalResult<FrequencyRangeTokens<'a>> {
    separated_pair(scalar_token, range_separator, scalar_token)
        .map(|(lower, upper)| FrequencyRangeTokens { lower, upper })
        .parse_next(input)
}

/// Grammar: `,` or `-` with optional leading whitespace.
fn range_separator(input: &mut &str) -> winnow::ModalResult<char> {
    preceded(multispace0, alt((',', '-'))).parse_next(input)
}

/// Grammar: optional whitespace, a numeric literal, then an optional unit suffix.
fn scalar_token<'a>(input: &mut &'a str) -> winnow::ModalResult<ScalarToken<'a>> {
    let number = preceded(multispace0, number_literal).parse_next(input)?;
    let unit = opt(unit_literal).parse_next(input)?.unwrap_or("");
    Ok(ScalarToken { number, unit })
}

/// Grammar: digits and underscores, optionally followed by `.` and more; validated later by [`scale_number`].
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

/// Grammar: an alphabetic/`/` suffix with optional leading whitespace; validated later by [`multiplier_for`].
fn unit_literal<'a>(input: &mut &'a str) -> winnow::ModalResult<&'a str> {
    preceded(
        multispace0,
        take_while(1.., |c: char| c.is_ascii_alphabetic() || c == '/'),
    )
    .parse_next(input)
}

/// Unit family a scalar is parsed against; selects the accepted suffixes and the base-unit label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitSet {
    Frequency,
    SampleRate,
}

impl UnitSet {
    /// Base-unit label used in diagnostics.
    const fn base_unit(self) -> &'static str {
        match self {
            Self::Frequency => "Hz",
            Self::SampleRate => "S/s",
        }
    }
}

/// Lower-case suffixes accepted for every unit family, with their multipliers.
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

/// Lower-case suffixes accepted only for sample rates, with their multipliers.
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

/// Look up the case-insensitive suffix `unit` in the tables `unit_set` allows.
fn multiplier_for(unit: &str, unit_set: UnitSet) -> Result<u64, ParseUnitError> {
    let normalized = unit.to_ascii_lowercase();
    find_multiplier(&normalized, FREQUENCY_UNITS)
        .or_else(|| match unit_set {
            UnitSet::Frequency => None,
            UnitSet::SampleRate => find_multiplier(&normalized, SAMPLE_RATE_UNITS),
        })
        .ok_or(ParseUnitError::UnknownUnit(normalized))
}

/// Find the multiplier for an already lower-cased suffix in one table.
fn find_multiplier(unit: &str, units: &[(&str, u64)]) -> Option<u64> {
    units
        .iter()
        .find_map(|(name, multiplier)| (*name == unit).then_some(*multiplier))
}

/// Multiply the literal `number` by `multiplier` exactly, rejecting results that
/// are fractional or overflow `u64`.
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
    let scale = u32::try_from(fractional.len())
        .ok()
        .and_then(|digits| 10_u128.checked_pow(digits))
        .ok_or_else(|| overflow_error(base_unit))?;

    let numerator = whole_value
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fractional_value))
        .and_then(|value| value.checked_mul(u128::from(multiplier)))
        .ok_or_else(|| overflow_error(base_unit))?;

    if numerator % scale != 0 {
        return Err(ParseUnitError::NonInteger(number.to_string()));
    }

    u64::try_from(numerator / scale).map_err(|_| overflow_error(base_unit))
}

/// Error for a scaled value that does not fit `u64`.
fn overflow_error(unit: &'static str) -> ParseUnitError {
    ParseUnitError::OutOfRange {
        value: u64::MAX,
        min: 0,
        max: u64::MAX,
        unit,
    }
}

/// Strip single interior underscores from one side of the decimal point; `original`
/// is reported on error.
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

/// Parse normalized digits as `u128`, treating an empty side as zero.
fn parse_u128_digits(input: &str, original: &str) -> Result<u128, ParseUnitError> {
    if input.is_empty() {
        return Ok(0);
    }
    input
        .parse::<u128>()
        .map_err(|_| ParseUnitError::InvalidNumber(original.to_string()))
}
