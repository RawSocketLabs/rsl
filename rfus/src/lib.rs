// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

#![warn(missing_docs)]

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
//! # Public API shape
//!
//! The crate-root re-exports are the public API. Internal files are split by
//! concept for maintainability, but callers should continue to import items
//! from `rfus` directly:
//!
//! - [`FrequencyHz`] for validated RF center frequencies.
//! - [`Hertz`] for unconstrained hertz quantities such as bandwidths.
//! - [`SampleRateSps`] for validated sample rates.
//! - [`FrequencyRange`] for ordered `lower < upper` frequency ranges.
//! - [`ScanTarget`] for either one static frequency or one or more ranges.
//! - [`ParseUnitError`] for parse and validation failures.
//!
//! # Parsing rules
//!
//! Numeric literals may include underscores between digits. Decimal literals
//! are accepted only when applying the suffix multiplier produces an exact
//! whole-number Hz or S/s value. Leading and trailing whitespace is ignored,
//! and whitespace may appear between a number and its unit suffix.
//!
//! Frequency ranges accept either `lower-upper` or `lower,upper`. Range lists
//! use semicolons, for example `400m-520m; 800m-900m`.
//!
//! # Formatting
//!
//! [`Display`](std::fmt::Display) implementations intentionally emit canonical
//! bare integer values, without unit suffixes. [`FrequencyRange`] displays as
//! `lower,upper`.
//!
//! # Serde
//!
//! With the `serde` feature enabled, scalar quantities deserialize from either
//! integer values or human-readable strings and serialize as integer values.
//! [`FrequencyRange`] deserializes from a range string or `{ lower, upper }`.
//! [`ScanTarget`] deserializes from a compact string, `{ static_frequency }`,
//! or `{ ranges }`.
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

mod error;
mod parser;

#[cfg(feature = "serde")]
mod serde_impl;

#[cfg(test)]
mod tests;

mod types;

pub use error::ParseUnitError;
pub use parser::{
    parse_frequency_hz, parse_frequency_range, parse_frequency_ranges, parse_hertz_u32,
    parse_sample_rate_sps,
};
pub use types::{FrequencyHz, FrequencyRange, Hertz, SampleRateSps, ScanTarget};
