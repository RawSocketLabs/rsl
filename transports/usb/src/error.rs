//! What can go wrong: [`Error`] for calls, [`TransferError`] for a queued transfer's outcome.

use std::io;
use std::path::PathBuf;

use rustix::io::Errno;

/// A call failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No device matched.
    #[error("no matching USB device")]
    NotFound,

    /// The device node is not writable by this process.
    #[error("permission denied opening {}: grant access with a udev rule", .0.display())]
    Access(PathBuf),

    /// The interface is claimed by another program, the endpoint already has a queue, or the
    /// device is still in use by handles that must be dropped first.
    #[error("busy: in use elsewhere")]
    Busy,

    /// The device went away.
    #[error("device disconnected")]
    Disconnected,

    /// The kernel refused memory for a transfer. usbfs caps all programs' transfer buffers
    /// together, 16 MiB by default.
    #[error(
        "the kernel refused transfer memory: raise /sys/module/usbcore/parameters/usbfs_memory_mb"
    )]
    NoMemory,

    /// The endpoint is not on this interface, or not in that direction.
    #[error("endpoint {0:#04x} is not on this interface in that direction")]
    InvalidEndpoint(u8),

    /// An IN transfer's length is not a nonzero multiple of the endpoint's packet size, or a
    /// transfer is longer than the kernel accepts.
    #[error("transfer length {len} is invalid for packets of {max_packet} bytes")]
    InvalidLength {
        /// The length asked for.
        len: usize,

        /// The endpoint's packet size.
        max_packet: usize,
    },

    /// A synchronous call did not complete in time.
    #[error("timed out")]
    TimedOut,

    /// The platform or device cannot do this.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    /// A transfer ended badly (a synchronous one, or a completion's status propagated).
    #[error(transparent)]
    Transfer(TransferError),

    /// Any other failure.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<TransferError> for Error {
    fn from(error: TransferError) -> Self {
        match error {
            TransferError::Disconnected => Self::Disconnected,
            other => Self::Transfer(other),
        }
    }
}

impl From<Errno> for Error {
    fn from(errno: Errno) -> Self {
        match errno {
            Errno::NODEV | Errno::SHUTDOWN => Self::Disconnected,
            Errno::BUSY => Self::Busy,
            Errno::NOMEM => Self::NoMemory,
            Errno::TIMEDOUT => Self::TimedOut,
            other => Self::Io(other.into()),
        }
    }
}

/// A queued transfer did not complete normally.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransferError {
    /// The endpoint stalled (halted).
    #[error("endpoint stalled")]
    Stall,

    /// The transfer was cancelled before it completed.
    #[error("cancelled")]
    Cancelled,

    /// The device went away.
    #[error("device disconnected")]
    Disconnected,

    /// The device sent more than the buffer holds.
    #[error("device sent more data than the buffer holds")]
    Overflow,

    /// Any other failure.
    #[error(transparent)]
    Io(io::Error),
}

impl TransferError {
    /// The outcome usbfs reports in a completed transfer's status: zero or a negated errno.
    pub(crate) fn from_status(status: i32) -> Result<(), Self> {
        if status == 0 {
            return Ok(());
        }
        Err(match Errno::from_raw_os_error(status.saturating_neg()) {
            Errno::PIPE => Self::Stall,
            Errno::NOENT | Errno::CONNRESET => Self::Cancelled,
            Errno::NODEV | Errno::SHUTDOWN => Self::Disconnected,
            Errno::OVERFLOW => Self::Overflow,
            other => Self::Io(other.into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_status_is_success_and_negated_errnos_classify() {
        assert!(TransferError::from_status(0).is_ok());
        let classified = |errno: Errno| TransferError::from_status(-errno.raw_os_error());
        assert!(matches!(classified(Errno::PIPE), Err(TransferError::Stall)));
        assert!(matches!(
            classified(Errno::NOENT),
            Err(TransferError::Cancelled)
        ));
        assert!(matches!(
            classified(Errno::CONNRESET),
            Err(TransferError::Cancelled)
        ));
        assert!(matches!(
            classified(Errno::SHUTDOWN),
            Err(TransferError::Disconnected)
        ));
        assert!(matches!(
            classified(Errno::OVERFLOW),
            Err(TransferError::Overflow)
        ));
        assert!(matches!(
            classified(Errno::PROTO),
            Err(TransferError::Io(_))
        ));
    }
}
