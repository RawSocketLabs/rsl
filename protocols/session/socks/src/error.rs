//! The crate's error type and result alias.
//!
//! Every fallible operation in this crate returns [`Result<T>`], which is
//! [`std::result::Result`] specialized to [`SocksError`]. The variants separate
//! transport failures ([`SocksError::Io`]), malformed wire data
//! ([`SocksError::MessageParse`]), protocol-level refusals the peer reported,
//! and local validation ([`SocksError::Validation`]).
//!
//! The type is **version-agnostic**: the variants that name a SOCKS5 reply code
//! ([`ReplyFailure`](SocksError::ReplyFailure), method negotiation) compile only
//! under the `v5` feature, and the SOCKS4 reply-code variant
//! ([`V4ReplyFailure`](SocksError::V4ReplyFailure)) only under `v4`, so the
//! error enum is exactly as wide as the versions you compiled in.
//!
//! [`io::Error`], [`AddrParseError`], and [`binrw::Error`] all convert into
//! `SocksError` via [`From`], so `?` works directly against the standard
//! library and the wire layer.

use std::io;
use std::net::AddrParseError;
use thiserror::Error;

/// Every error this crate can produce. See the [module docs](crate::error) for
/// how the variants are grouped.
#[derive(Error, Debug)]
pub enum SocksError {
    /// I/O errors that occur during network operations
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Errors that occur when parsing network addresses
    #[error("Failed to parse address: {0}")]
    AddressParse(#[from] AddrParseError),

    /// Errors that occur during message parsing
    #[error("Failed to parse SOCKS message: {0}")]
    MessageParse(String),

    /// Errors that occur during message construction
    #[error("Failed to construct SOCKS message: {0}")]
    MessageConstruction(String),

    /// Errors that occur when the peer speaks an unexpected protocol version
    #[error("Unsupported SOCKS version: {0}")]
    UnsupportedVersion(u8),

    /// Errors that occur when method negotiation finds no common method
    /// (SOCKS5 only).
    #[cfg(feature = "v5")]
    #[error("No acceptable authentication methods")]
    NoAcceptableMethods,

    /// Errors that occur when an authentication subnegotiation fails
    /// (SOCKS5 only).
    #[cfg(feature = "v5")]
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Errors that occur when the SOCKS5 server replies with a non-success
    /// code.
    #[cfg(feature = "v5")]
    #[error("Server replied with failure: {0:?}")]
    ReplyFailure(crate::v5::Response),

    /// Errors that occur when the SOCKS4 server returns a non-granted reply
    /// code (90 is granted; 91–93 and any other are failures).
    #[cfg(feature = "v4")]
    #[error("SOCKS4 server rejected the request: {0:?}")]
    V4ReplyFailure(crate::v4::ReplyCode),

    /// Errors that occur when validation fails
    #[error("Validation error: {0}")]
    Validation(String),

    /// Errors that occur when a feature is not supported
    #[error("Feature not supported: {0}")]
    NotSupported(String),
}

/// A type alias for Result that uses SocksError as the error type
pub type Result<T> = std::result::Result<T, SocksError>;

impl From<binrw::Error> for SocksError {
    fn from(err: binrw::Error) -> Self {
        match err {
            binrw::Error::Io(e) => SocksError::Io(e),
            binrw::Error::BadMagic { .. } => {
                SocksError::MessageParse("Invalid magic bytes".to_string())
            }
            binrw::Error::NoVariantMatch { .. } => {
                SocksError::MessageParse("No matching variant".to_string())
            }
            binrw::Error::Custom { err, .. } => SocksError::MessageParse(err.to_string()),
            _ => SocksError::MessageParse("Unknown parsing error".to_string()),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::other("test error");
        let socks_err = SocksError::from(io_err);
        assert!(matches!(socks_err, SocksError::Io(_)));
    }

    #[test]
    fn test_error_display() {
        let err = SocksError::UnsupportedVersion(4);
        assert_eq!(err.to_string(), "Unsupported SOCKS version: 4");
    }
}
