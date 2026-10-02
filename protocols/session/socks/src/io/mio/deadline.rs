// --- Standard library ---
use std::time::{Duration, Instant};

// --- Internal modules ---
use crate::error::Error;
pub(crate) use crate::io::deadline::deadline;

pub(crate) fn remaining(until: Instant, now: Instant) -> Result<Duration, Error> {
    Ok(crate::io::deadline::remaining(until, now)?)
}
