use crate::ffi;
use std::ffi::c_int;
use thiserror::Error;

/// A native or receive-adapter failure, retaining unknown native status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    /// Invalid parameter.
    #[error("invalid parameter")]
    InvalidParameter,
    /// Device not found.
    #[error("device not found")]
    NotFound,
    /// Device is busy.
    #[error("device is busy")]
    Busy,
    /// Memory allocation failed.
    #[error("memory allocation failed")]
    NoMemory,
    /// Operation is unsupported.
    #[error("operation is unsupported")]
    Unsupported,
    /// Usb operation failed.
    #[error("USB operation failed")]
    LibUsb,
    /// Thread operation failed.
    #[error("thread operation failed")]
    Thread,
    /// Streaming thread failed.
    #[error("streaming thread failed")]
    StreamingThread,
    /// Streaming has stopped.
    #[error("streaming has stopped")]
    StreamingStopped,
    /// Unspecified libairspy failure.
    #[error("unspecified libairspy failure")]
    Other,
    /// No samples arrived within the receive deadline.
    #[error("receive timed out")]
    Timeout,
    /// The receive buffer filled; stop and restart reception to recover.
    #[error("receive buffer overflow; restart reception")]
    Overflow,
    /// A status not recognized by this wrapper.
    #[error("unknown libairspy error code {0}")]
    Unknown(c_int),
}

/// Result of a libairspy operation.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Convert a native operation status; only zero means success.
    pub(crate) fn check(code: c_int) -> Result<()> {
        match code {
            ffi::AIRSPY_SUCCESS => Ok(()),
            ffi::AIRSPY_ERROR_INVALID_PARAM => Err(Self::InvalidParameter),
            ffi::AIRSPY_ERROR_NOT_FOUND => Err(Self::NotFound),
            ffi::AIRSPY_ERROR_BUSY => Err(Self::Busy),
            ffi::AIRSPY_ERROR_NO_MEM => Err(Self::NoMemory),
            ffi::AIRSPY_ERROR_UNSUPPORTED => Err(Self::Unsupported),
            ffi::AIRSPY_ERROR_LIBUSB => Err(Self::LibUsb),
            ffi::AIRSPY_ERROR_THREAD => Err(Self::Thread),
            ffi::AIRSPY_ERROR_STREAMING_THREAD_ERR => Err(Self::StreamingThread),
            ffi::AIRSPY_ERROR_STREAMING_STOPPED => Err(Self::StreamingStopped),
            ffi::AIRSPY_ERROR_OTHER => Err(Self::Other),
            code => Err(Self::Unknown(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_statuses_are_matchable_and_preserved() {
        assert_eq!(Error::check(0), Ok(()));
        let cases = [
            (-2, Error::InvalidParameter),
            (-5, Error::NotFound),
            (-6, Error::Busy),
            (-11, Error::NoMemory),
            (-12, Error::Unsupported),
            (-1000, Error::LibUsb),
            (-1001, Error::Thread),
            (-1002, Error::StreamingThread),
            (-1003, Error::StreamingStopped),
            (-9999, Error::Other),
            (-12345, Error::Unknown(-12345)),
            (1, Error::Unknown(1)),
        ];
        for (code, error) in cases {
            assert_eq!(Error::check(code), Err(error));
        }
    }
}
