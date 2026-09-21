use crate::error::Error;
use bnb::BitEncode;
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
pub(crate) async fn write<S: AsyncWrite + Unpin, T: BitEncode>(
    stream: &mut S,
    message: &T,
) -> Result<(), Error> {
    stream
        .write_all(&bnb::bitstream::encode_to_vec(message, T::LAYOUT)?)
        .await?;
    stream.flush().await?;
    Ok(())
}

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
