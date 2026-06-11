//! The crate's error type and result alias.
//!
//! Every fallible operation in this crate returns [`Result<T>`], which is
//! [`std::result::Result`] specialized to [`SocksError`]. The variants separate
//! transport failures ([`SocksError::Io`]), malformed wire data
//! ([`SocksError::MessageParse`]), protocol-level refusals the peer reported
//! ([`SocksError::ReplyFailure`], [`SocksError::NoAcceptableMethods`],
//! [`SocksError::AuthenticationFailed`]), and local validation
//! ([`SocksError::Validation`]).
//!
//! [`io::Error`], [`AddrParseError`], and [`binrw::Error`] all convert into
//! `SocksError` via [`From`], so `?` works directly against the standard
//! library and the wire layer.

use std::io;
use std::net::AddrParseError;
use thiserror::Error;

use crate::v5::Response;

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
    #[error("No acceptable authentication methods")]
    NoAcceptableMethods,

    /// Errors that occur when an authentication subnegotiation fails
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Errors that occur when the server replies with a non-success code
    #[error("Server replied with failure: {0:?}")]
    ReplyFailure(Response),

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
        let io_err = io::Error::new(io::ErrorKind::Other, "test error");
        let socks_err = SocksError::from(io_err);
        assert!(matches!(socks_err, SocksError::Io(_)));
    }

    #[test]
    fn test_error_display() {
        let err = SocksError::UnsupportedVersion(4);
        assert_eq!(err.to_string(), "Unsupported SOCKS version: 4");
    }
}
