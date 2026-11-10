//! What can go wrong.

use hal::BusError;

use crate::portable::pending::Gap;

/// A driver operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The operation needs a value no admissible source gives yet.
    #[error(transparent)]
    Gap(#[from] Gap),

    /// A sample rate outside the evidenced ranges (`RTL-OBS-12`).
    #[error("sample rate {0} S/s is outside the evidenced ranges")]
    Rate(u32),

    /// A frequency whose LO is outside the evidenced range (`RTL-OBS-15`).
    #[error("frequency {0} Hz is outside the evidenced tuning range")]
    Frequency(u32),

    /// Tuning before a sample rate is set: the rate fixes the IF.
    #[error("no sample rate set")]
    NoRate,

    /// The tuner's PLL did not lock, even after the retry (`RTL-OBS-16`).
    #[error("the tuner's PLL did not lock")]
    Unlocked,

    /// The transport failed.
    #[error("transport failed")]
    Bus(#[from] BusError),
}
