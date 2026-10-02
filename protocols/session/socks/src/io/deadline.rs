// --- Standard library ---
use std::time::{Duration, Instant};

// --- Internal modules ---
use crate::error::Error;

/// Reject unusable budgets and compute a deadline without depending on a transport.
pub(crate) fn deadline(now: Instant, duration: Duration) -> Result<Instant, Error> {
    if duration.is_zero() {
        Err(Error::InvalidLimits)
    } else {
        now.checked_add(duration).ok_or(Error::InvalidLimits)
    }
}

#[cfg(feature = "blocking")]
pub(crate) fn remaining(until: Instant, now: Instant) -> std::io::Result<Duration> {
    until
        .checked_duration_since(now)
        .filter(|duration| !duration.is_zero())
        .ok_or_else(timed_out)
}

#[cfg(any(feature = "blocking", feature = "tokio"))]
pub(crate) fn timed_out() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "SOCKS deadline expired")
}
