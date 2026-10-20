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

    /// The transport failed.
    #[error("transport failed")]
    Bus(#[from] BusError),
}
