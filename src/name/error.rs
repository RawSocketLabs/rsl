use thiserror::Error;

use crate::name::codes::RValue;

/// The common response codes and defintions that can be mapped to from a specific RValue.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RError {
    /// Success
    ///
    /// Valid for the following response types: `Query`, `Release`, `Registration`
    #[error("Success")]
    Success,

    /// Format: Request was invalidly formatted.
    ///
    /// Valid for the following response types: `Query`, `Release`, `Registration`
    #[error("Format: Request was invalidly formatted.")]
    FormatError,

    /// Server Failure: Problem with NBNS, cannot process name.
    ///
    /// Valid for the following response types: `Query`, `Release`, `Registration`
    #[error("Server Failure: Problem with NBNS, cannot process name.")]
    ServerFailure,

    /// Name: The name requested does not exist.
    ///
    /// Valid for the following response types: `Query`
    #[error("Name: The name requested does not exist.")]
    NameError,

    /// Unsupported Request: Allowable only for challenging NBNS when gets an Update type registration request.
    ///
    /// Valid for the following response types: `Query`, `Registration`
    #[error("Unsupported Request: Allowable only for challenging NBNS when gets an Update type registration request.")]
    UnsupportedRequest,

    /// Refused: For policy reasons server will not register this name from this host.
    ///
    /// Valid for the following response types: `Query`, `Release`, `Registration`
    #[error("Refused: For policy reasons server will not register this name from this host.")]
    Refused,

    /// Active: Name is owned by another node.
    ///
    /// Valid for the following response types: `Release`, `Registration`
    #[error("Active: Name is owned by another node.")]
    ActiveError,

    /// Conflict: Name is owned by another node.
    ///
    /// Valid for the following response types: `Registration`
    #[error("Conflict: Name is owned by another node.")]
    ConflictError,

    /// Unknown: Unknown response code `number`.
    #[error("Unknown: Unknown response code {0}.")]
    Unknown(u8),
}

impl From<RValue> for RError {
    /// Converts an RValue to an RError.
    fn from(rvalue: RValue) -> Self {
        Self::Unknown(rvalue as u8)
    }
}

#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingError {
    /// The name provided must contain at least one character.
    #[error("An empty name was proviced. Names must be at least one character long.")]
    EmptyName,

    /// The name is too long to be encoded as a NetBIOS name.
    #[error("The name is too long to be encoded as a NetBIOS name.")]
    NameTooLong,

    #[error("The name contains an invalid character: {0}.")]
    InvalidCharacter(char),

    #[error("The scope id did not contain exactly two portions split by a period.")]
    InvalidScopeId,
}

#[cfg(test)]
mod unit {
    use super::*;

    use crate::name::codes::{QueryCode, RegistrationCode, ReleaseCode};

    #[test]
    fn query_error() {
        let error: RError = QueryCode::ServerFailure.into();

        assert_eq!(error, RError::ServerFailure);
    }

    #[test]
    fn release_error() {
        let error: RError = ReleaseCode::Refused.into();

        assert_eq!(error, RError::Refused);
    }

    #[test]
    fn registration_error() {
        let error: RError = RegistrationCode::ConflictError.into();

        assert_eq!(error, RError::ConflictError);
    }

    #[test]
    fn custom_error() {
        let error: RError = RValue::Ten.into();

        assert_eq!(error, RError::Unknown(10));
    }
}
