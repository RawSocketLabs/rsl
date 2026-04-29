// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

/// Lowest accepted RF frequency for [`FrequencyHz`](crate::FrequencyHz).
///
/// Human-facing frequency parsing rejects values below this floor, while
/// [`Hertz`](crate::Hertz) remains available for unconstrained quantities such
/// as bandwidths and offsets.
pub const MIN_FREQUENCY_HZ: u64 = 1_000_000;

/// Highest accepted RF frequency for [`FrequencyHz`](crate::FrequencyHz).
///
/// This is currently `u64::MAX`, so the practical upper bound is integer
/// overflow during exact decimal scaling.
pub const MAX_FREQUENCY_HZ: u64 = u64::MAX;

/// Lowest accepted sample rate for [`SampleRateSps`](crate::SampleRateSps).
pub const MIN_SAMPLE_RATE_SPS: u32 = 200_000;

/// Highest accepted sample rate for [`SampleRateSps`](crate::SampleRateSps).
pub const MAX_SAMPLE_RATE_SPS: u32 = 20_000_000;
