// RawSocket Labs LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

use std::str::FromStr;

use winnow::Parser;
use winnow::ascii::multispace0;
use winnow::combinator::{alt, eof, opt, preceded, separated, separated_pair, terminated};
use winnow::error::{ContextError, ErrMode};
use winnow::token::{one_of, take_while};

use crate::error::ParseUnitError;
use crate::types::{FrequencyHz, FrequencyRange, Hertz, SampleRateSps, ScanTarget};

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

pub(crate) fn parse_frequency_value(input: &str) -> Result<u64, ParseUnitError> {
    parse_unit_value(input, UnitSet::Frequency)
}

pub(crate) fn parse_sample_rate_value(input: &str) -> Result<u64, ParseUnitError> {
    parse_unit_value(input, UnitSet::SampleRate)
}

pub(crate) fn parse_frequency_range_value(input: &str) -> Result<FrequencyRange, ParseUnitError> {
    parse_complete(input, frequency_range_tokens)?.resolve()
}

pub(crate) fn parse_scan_target_value(input: &str) -> Result<ScanTarget, ParseUnitError> {
    parse_complete(input, scan_target_tokens)?.resolve()
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
