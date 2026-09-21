use crate::error::Error;
use std::time::{Duration, Instant};

pub(crate) use crate::io::deadline::deadline;

pub(crate) fn remaining(until: Instant, now: Instant) -> Result<Duration, Error> {
    Ok(crate::io::deadline::remaining(until, now)?)
}
