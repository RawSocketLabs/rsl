use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::error::ParseUnitError;
use crate::parser::{self, UnitSet};

/// A generic hertz quantity.
///
/// Use this for bandwidths, offsets, and other Hz values. It carries the
/// [`Hertz::as_u32`] helper for hardware APIs that need a `u32`; semantically it
/// reads as a raw quantity rather than an RF tuning frequency ([`FrequencyHz`]).
/// Display formatting emits the bare integer Hz value.
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
    #[must_use]
    pub const fn hz(self) -> u64 {
        self.0
    }

    /// Return the quantity as `u32` when required by hardware APIs.
    ///
    /// # Errors
    ///
    /// Returns [`ParseUnitError::OutOfRange`] when the value is larger than
    /// `u32::MAX`.
    pub fn as_u32(self) -> Result<u32, ParseUnitError> {
        fit_u32(self.0, "Hz")
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

/// An RF frequency in Hz.
///
/// Parsing imposes no RF range bounds — any value that parses to a whole Hz
/// integer is accepted. Range limits (e.g. what a device can tune to) are the
/// consumer's responsibility. Display formatting emits the bare integer Hz
/// value.
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
    /// Create a frequency from a raw Hz value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the frequency in whole hertz.
    #[must_use]
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
        Ok(Self::new(hz))
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
    /// Create a sample rate from a raw S/s value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the sample rate in whole samples per second.
    #[must_use]
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
        // Only a type-fit check: the value must fit u32. No RF range bounds.
        fit_u32(value, "S/s").map(Self)
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
/// assert_eq!(range.lower().hz(), 400_000_000);
/// assert_eq!(range.upper().hz(), 520_000_000);
/// assert_eq!(range.canonical(), "400000000,520000000");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrequencyRange {
    /// Lower bound in Hz; always below `upper`.
    lower: FrequencyHz,
    /// Upper bound in Hz; always above `lower`.
    upper: FrequencyHz,
}

impl FrequencyRange {
    /// Create a range and validate that `lower < upper`.
    ///
    /// # Errors
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

    /// Return the lower bound, which is always below [`FrequencyRange::upper`].
    #[must_use]
    pub const fn lower(&self) -> FrequencyHz {
        self.lower
    }

    /// Return the upper bound, which is always above [`FrequencyRange::lower`].
    #[must_use]
    pub const fn upper(&self) -> FrequencyHz {
        self.upper
    }

    /// Return the canonical wire/debug form: `lower,upper` in integer Hz.
    #[must_use]
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

/// Narrow a parsed `u64` quantity to `u32`, reporting the `unit` on overflow.
///
/// Used by the `u32`-backed accessors and types; it applies no RF bounds.
fn fit_u32(value: u64, unit: &'static str) -> Result<u32, ParseUnitError> {
    u32::try_from(value).map_err(|_| ParseUnitError::OutOfRange {
        value,
        min: 0,
        max: u64::from(u32::MAX),
        unit,
    })
}
