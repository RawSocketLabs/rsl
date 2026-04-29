// Metrea LLC Intellectual Property
// Originally developed by Raw Socket Labs LLC

/// Lowest accepted RF frequency for user-facing frequency inputs.
pub const MIN_FREQUENCY_HZ: u64 = 1_000_000;
/// Highest accepted RF frequency for user-facing frequency inputs.
pub const MAX_FREQUENCY_HZ: u64 = u64::MAX;
/// Lowest accepted sample rate in samples per second.
pub const MIN_SAMPLE_RATE_SPS: u32 = 200_000;
/// Highest accepted sample rate in samples per second.
pub const MAX_SAMPLE_RATE_SPS: u32 = 20_000_000;
