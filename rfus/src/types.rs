// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::constants::{
    MAX_FREQUENCY_HZ, MAX_SAMPLE_RATE_SPS, MIN_FREQUENCY_HZ, MIN_SAMPLE_RATE_SPS,
};
use crate::error::ParseUnitError;
use crate::parser::{self, UnitSet};

/// A generic hertz quantity.
///
/// Use this for bandwidths, offsets, and other Hz values that should not be
/// constrained by the RF frequency floor. Display formatting emits the bare
/// integer Hz value.
///
/// # Examples
///
/// ```
/// let bandwidth: rfus::Hertz = "12.5khz".parse().unwrap();
/// assert_eq!(bandwidth.hz(), 12_500);
/// assert_eq!(bandwidth.to_string(), "12500");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hertz(u64);

impl Hertz {
    /// Return the quantity in whole hertz.
    pub const fn hz(self) -> u64 {
        self.0
    }

    /// Return the quantity as `u32` when required by hardware APIs.
    ///
    /// Returns [`ParseUnitError::OutOfRange`] when the value is larger than
    /// `u32::MAX`.
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
        Ok(Self(parser::parse_unit_value(input, UnitSet::Frequency)?))
    }
}

/// A validated RF frequency in Hz.
///
/// Parsing enforces [`crate::MIN_FREQUENCY_HZ`] and
/// [`crate::MAX_FREQUENCY_HZ`]. Use [`FrequencyHz::new_unchecked`] only at
/// trusted boundaries where validation has already happened. Display formatting
/// emits the bare integer Hz value.
///
/// # Examples
///
/// ```
/// let frequency: rfus::FrequencyHz = "450.5 MHz".parse().unwrap();
/// assert_eq!(frequency.hz(), 450_500_000);
/// assert_eq!(frequency.to_string(), "450500000");
/// ```
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

    /// Create a validated frequency from a raw Hz value.
    ///
    /// Returns [`ParseUnitError::OutOfRange`] when `value` is outside
    /// [`crate::MIN_FREQUENCY_HZ`] through [`crate::MAX_FREQUENCY_HZ`].
    pub fn new(value: u64) -> Result<Self, ParseUnitError> {
        validate_range(value, MIN_FREQUENCY_HZ, MAX_FREQUENCY_HZ, "Hz")?;
        Ok(Self(value))
    }

    /// Return the frequency in whole hertz.
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
        let hz = parser::parse_unit_value(input, UnitSet::Frequency)?;
        Self::new(hz)
    }
}

/// A sample rate in samples per second.
///
/// Sample rates accept both frequency-style units (`2MHz`) and sample-rate
/// units (`2MS/s`, `2msps`) because hardware and CLI users commonly use both.
/// Display formatting emits the bare integer S/s value.
///
/// # Examples
///
/// ```
/// let rate: rfus::SampleRateSps = "2 MS/s".parse().unwrap();
/// assert_eq!(rate.sps(), 2_000_000);
/// assert_eq!(rate.to_string(), "2000000");
/// ```
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

    /// Create a validated sample rate from a raw S/s value.
    ///
    /// Returns [`ParseUnitError::OutOfRange`] when `value` is outside
    /// [`crate::MIN_SAMPLE_RATE_SPS`] through [`crate::MAX_SAMPLE_RATE_SPS`].
    pub fn new(value: u32) -> Result<Self, ParseUnitError> {
        validate_range(
            value as u64,
            MIN_SAMPLE_RATE_SPS as u64,
            MAX_SAMPLE_RATE_SPS as u64,
            "S/s",
        )?;
        Ok(Self(value))
    }

    /// Return the sample rate in whole samples per second.
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
        let value = parser::parse_unit_value(input, UnitSet::SampleRate)?;
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
///
/// # Examples
///
/// ```
/// let range: rfus::FrequencyRange = "400m-520m".parse().unwrap();
/// assert_eq!(range.lower.hz(), 400_000_000);
/// assert_eq!(range.upper.hz(), 520_000_000);
/// assert_eq!(range.canonical(), "400000000,520000000");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrequencyRange {
    /// Lower bound in Hz.
    pub lower: FrequencyHz,
    /// Upper bound in Hz.
    pub upper: FrequencyHz,
}

impl FrequencyRange {
    /// Create a range and validate that `lower < upper`.
    ///
    /// Returns [`ParseUnitError::InvalidRange`] when the endpoints are equal or
    /// reversed.
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
        parser::parse_frequency_range(input)
    }
}

/// A scan target provided by the user.
///
/// A single frequency such as `450m` becomes [`ScanTarget::Static`]. One or
/// more ranges such as `1mhz-20mhz; 400m-520m` become [`ScanTarget::Ranges`].
///
/// # Examples
///
/// ```
/// let static_target: rfus::ScanTarget = "450m".parse().unwrap();
/// assert!(matches!(static_target, rfus::ScanTarget::Static(_)));
///
/// let ranged_target: rfus::ScanTarget = "400m-520m; 800m-900m".parse().unwrap();
/// assert!(matches!(ranged_target, rfus::ScanTarget::Ranges(_)));
/// ```
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
        parser::parse_scan_target(input)
    }
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
