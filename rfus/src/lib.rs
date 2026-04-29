// RawSocket Labs LLC Intellectual Property
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

mod constants;
mod error;
mod parser;

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(test)]
mod tests;

mod types;
mod validation;

pub use constants::{MAX_FREQUENCY_HZ, MAX_SAMPLE_RATE_SPS, MIN_FREQUENCY_HZ, MIN_SAMPLE_RATE_SPS};
pub use error::ParseUnitError;
pub use parser::{
    parse_frequency_hz, parse_frequency_range, parse_frequency_ranges, parse_hertz_u32,
    parse_sample_rate_sps,
};
pub use types::{FrequencyHz, FrequencyRange, Hertz, SampleRateSps, ScanTarget};
