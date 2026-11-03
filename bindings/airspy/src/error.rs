use std::ffi::c_int;
use thiserror::Error;

/// A libairspy failure, retaining unknown native status codes.
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
            0 => Ok(()),
            -2 => Err(Self::InvalidParameter),
            -5 => Err(Self::NotFound),
            -6 => Err(Self::Busy),
            -11 => Err(Self::NoMemory),
            -12 => Err(Self::Unsupported),
            -1000 => Err(Self::LibUsb),
            -1001 => Err(Self::Thread),
            -1002 => Err(Self::StreamingThread),
            -1003 => Err(Self::StreamingStopped),
            -9999 => Err(Self::Other),
            code => Err(Self::Unknown(code)),
        }
    }

    /// Return the original native status code.
    pub fn code(self) -> c_int {
        match self {
            Self::InvalidParameter => -2,
            Self::NotFound => -5,
            Self::Busy => -6,
            Self::NoMemory => -11,
            Self::Unsupported => -12,
            Self::LibUsb => -1000,
            Self::Thread => -1001,
            Self::StreamingThread => -1002,
            Self::StreamingStopped => -1003,
            Self::Other => -9999,
            Self::Unknown(code) => code,
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
            assert_eq!(error.code(), code);
        }
    }
}
