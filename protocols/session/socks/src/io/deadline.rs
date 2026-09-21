#[cfg(any(feature = "blocking", feature = "mio"))]
use crate::error::Error;
#[cfg(any(feature = "blocking", feature = "mio"))]
use std::time::{Duration, Instant};

#[cfg(any(feature = "blocking", feature = "mio"))]
pub(crate) fn deadline(now: Instant, duration: Duration) -> Result<Instant, Error> {
    if duration.is_zero() {
        Err(Error::InvalidLimits)
    } else {
        now.checked_add(duration).ok_or(Error::InvalidLimits)
    }
}

#[cfg(any(feature = "blocking", feature = "mio"))]
pub(crate) fn remaining(until: Instant, now: Instant) -> std::io::Result<Duration> {
    until
        .checked_duration_since(now)
        .filter(|duration| !duration.is_zero())
        .ok_or_else(timed_out)
}

pub(crate) fn timed_out() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "SOCKS deadline expired")
}
