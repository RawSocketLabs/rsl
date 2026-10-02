// --- Standard library ---
use std::{
    future::Future,
    time::{Duration, Instant},
};

// --- Internal modules ---
use crate::error::Error;
pub(crate) async fn bounded<T>(
    duration: Duration,
    work: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    if duration.is_zero() || Instant::now().checked_add(duration).is_none() {
        return Err(Error::InvalidLimits);
    }
    tokio::time::timeout(duration, work)
        .await
        .map_err(|_| Error::Io(crate::io::timed_out()))?
}
pub(crate) async fn bounded_until<T>(
    until: tokio::time::Instant,
    work: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    check_deadline(until)?;
    tokio::time::timeout_at(until, work)
        .await
        .map_err(|_| Error::Io(crate::io::timed_out()))?
}

pub(crate) fn check_deadline(until: tokio::time::Instant) -> Result<(), Error> {
    if tokio::time::Instant::now() >= until {
        Err(crate::io::timed_out().into())
    } else {
        Ok(())
    }
}
