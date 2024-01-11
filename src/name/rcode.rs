use modular_bitfield::prelude::*;

use crate::name::codes::Op;
use crate::name::{RError, State};

/// Indicates the result of a request.
#[derive(Debug, Clone, Copy)]
pub enum RCode {
    /// Contains all valid responses to a query.
    Query(QueryCode),

    /// Contains all valid responses to a release.
    Release(ReleaseCode),

    /// Contains all valid responses to a registration.
    Registration(RegistrationCode),

    /// Contains a custom response code which covers the entire 4 bit range.
    Custom(RValue),
}

/// The valid response codes for a query.
#[derive(Debug, Clone, Copy)]
pub enum QueryCode {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    UnsupportedRequest = 4,
    Refused = 5,
}

impl From<QueryCode> for RValue {
    /// Converts a Query to an RValue.
    fn from(query: QueryCode) -> Self {
        match query {
            QueryCode::Success => Self::Zero,
            QueryCode::FormatError => Self::One,
            QueryCode::ServerFailure => Self::Two,
            QueryCode::NameError => Self::Three,
            QueryCode::UnsupportedRequest => Self::Four,
            QueryCode::Refused => Self::Five,
        }
    }
}

impl From<QueryCode> for RError {
    /// Converts a Query to an RError.
    fn from(query: QueryCode) -> Self {
        match query {
            QueryCode::Success => Self::Success,
            QueryCode::FormatError => Self::FormatError,
            QueryCode::ServerFailure => Self::ServerFailure,
            QueryCode::NameError => Self::NameError,
            QueryCode::UnsupportedRequest => Self::UnsupportedRequest,
            QueryCode::Refused => Self::Refused,
        }
    }
}

impl From<QueryCode> for RCode {
    /// Converts a Query to an RCode.
    fn from(query: QueryCode) -> Self {
        Self::Query(query)
    }
}

/// The valid response codes for a release.
#[derive(Debug, Clone, Copy)]
pub enum ReleaseCode {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    Refused = 5,
    ActiveError = 6,
}

impl From<ReleaseCode> for RValue {
    /// Maps a Release to an RValue.
    fn from(release: ReleaseCode) -> Self {
        match release {
            ReleaseCode::Success => Self::Zero,
            ReleaseCode::FormatError => Self::One,
            ReleaseCode::ServerFailure => Self::Two,
            ReleaseCode::Refused => Self::Five,
            ReleaseCode::ActiveError => Self::Six,
        }
    }
}

impl From<ReleaseCode> for RError {
    /// Maps a Release to an RError.
    fn from(release: ReleaseCode) -> Self {
        match release {
            ReleaseCode::Success => Self::Success,
            ReleaseCode::FormatError => Self::FormatError,
            ReleaseCode::ServerFailure => Self::ServerFailure,
            ReleaseCode::Refused => Self::Refused,
            ReleaseCode::ActiveError => Self::ActiveError,
        }
    }
}

impl From<ReleaseCode> for RCode {
    /// Maps a Release to an RCode.
    fn from(release: ReleaseCode) -> Self {
        Self::Release(release)
    }
}

/// The valid response codes for a registration.
#[derive(Debug, Clone, Copy)]
pub enum RegistrationCode {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    UnsupportedRequest = 4,
    Refused = 5,
    ActiveError = 6,
    ConflictError = 7,
}

impl From<RegistrationCode> for RValue {
    /// Maps a Registration to an RValue.
    fn from(registration: RegistrationCode) -> Self {
        match registration {
            RegistrationCode::Success => Self::Zero,
            RegistrationCode::FormatError => Self::One,
            RegistrationCode::ServerFailure => Self::Two,
            RegistrationCode::UnsupportedRequest => Self::Four,
            RegistrationCode::Refused => Self::Five,
            RegistrationCode::ActiveError => Self::Six,
            RegistrationCode::ConflictError => Self::Seven,
        }
    }
}

impl From<RegistrationCode> for RError {
    /// Maps a Registration to an RError.
    fn from(registration: RegistrationCode) -> Self {
        match registration {
            RegistrationCode::Success => Self::Success,
            RegistrationCode::FormatError => Self::FormatError,
            RegistrationCode::ServerFailure => Self::ServerFailure,
            RegistrationCode::UnsupportedRequest => Self::UnsupportedRequest,
            RegistrationCode::Refused => Self::Refused,
            RegistrationCode::ActiveError => Self::ActiveError,
            RegistrationCode::ConflictError => Self::ConflictError,
        }
    }
}

impl From<RegistrationCode> for RCode {
    /// Maps a Registration to an RCode.
    fn from(registration: RegistrationCode) -> Self {
        Self::Registration(registration)
    }
}

/// The underlying response value. Can map to an appopriate `RCode`.
///
/// `RValue`s should only be utilized directly when specifying a `RCode::Custom`.
/// Using `RValue.into()` will convert the `RValue` directly into a `RCode::Custom`.
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
#[bits = 4]
pub enum RValue {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Eleven = 11,
    Twelve = 12,
    Thirteen = 13,
    Fourteen = 14,
    Fifteen = 15,
}

impl From<RValue> for RCode {
    /// Converts an `RValue` into an `RCode`. The converted value is always of type `RCode::Custom`.
    fn from(rvalue: RValue) -> Self {
        RCode::Custom(rvalue)
    }
}

impl RCode {
    /// Maps a response code to a query response code.
    pub fn query(code: RValue) -> Self {
        match code {
            RValue::Zero => Self::Query(QueryCode::Success),
            RValue::One => Self::Query(QueryCode::FormatError),
            RValue::Two => Self::Query(QueryCode::ServerFailure),
            RValue::Three => Self::Query(QueryCode::NameError),
            RValue::Four => Self::Query(QueryCode::UnsupportedRequest),
            RValue::Five => Self::Query(QueryCode::Refused),
            r => Self::Custom(r),
        }
    }

    /// Maps a response code to a release response code.
    pub fn release(code: RValue) -> Self {
        match code {
            RValue::Zero => Self::Release(ReleaseCode::Success),
            RValue::One => Self::Release(ReleaseCode::FormatError),
            RValue::Two => Self::Release(ReleaseCode::ServerFailure),
            RValue::Five => Self::Release(ReleaseCode::Refused),
            RValue::Six => Self::Release(ReleaseCode::ActiveError),
            r => Self::Custom(r),
        }
    }

    /// Maps a response code to a registration response code.
    pub fn registration(code: RValue) -> Self {
        match code {
            RValue::Zero => Self::Registration(RegistrationCode::Success),
            RValue::One => Self::Registration(RegistrationCode::FormatError),
            RValue::Two => Self::Registration(RegistrationCode::ServerFailure),
            RValue::Four => Self::Registration(RegistrationCode::UnsupportedRequest),
            RValue::Five => Self::Registration(RegistrationCode::Refused),
            RValue::Six => Self::Registration(RegistrationCode::ActiveError),
            RValue::Seven => Self::Registration(RegistrationCode::ConflictError),
            r => Self::Custom(r),
        }
    }
}

impl RCode {
    /// Returns the apporpriate response code for the given state.
    pub(crate) fn from_state(state: State) -> Self {
        match (state.opcode().op(), state.rcode()) {
            (Op::Query, rcode) => Self::query(rcode),
            (Op::Registration, rcode) => Self::registration(rcode),
            (Op::Release, rcode) => Self::release(rcode),
            (_, rcode) => Self::Custom(rcode),
        }
    }
}

impl From<RCode> for RValue {
    /// Converts an RCode to an RValue.
    fn from(rtype: RCode) -> Self {
        match rtype {
            RCode::Query(query) => query.into(),
            RCode::Release(release) => release.into(),
            RCode::Registration(registration) => registration.into(),
            RCode::Custom(rcode) => rcode,
        }
    }
}

impl From<RCode> for RError {
    /// Converts an RCode to an RError.
    fn from(rtype: RCode) -> Self {
        match rtype {
            RCode::Query(query) => query.into(),
            RCode::Release(release) => release.into(),
            RCode::Registration(registration) => registration.into(),
            RCode::Custom(rvalue) => rvalue.into(),
        }
    }
}

#[cfg(test)]
mod unit {}
