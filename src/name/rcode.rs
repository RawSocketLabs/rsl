use modular_bitfield::prelude::*;

use crate::name::{Op, RError, State};

/// Indicates the result of a request.
#[derive(Debug, Clone, Copy)]
pub enum RCode {
    /// Contains all valid responses to a query.
    Query(Query),

    /// Contains all valid responses to a release.
    Release(Release),

    /// Contains all valid responses to a registration.
    Registration(Registration),

    /// Contains a custom response code which covers the entire 4 bit range.
    Custom(RValue),
}

/// The valid response codes for a query.
#[derive(Debug, Clone, Copy)]
pub enum Query {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    UnsupportedRequest = 4,
    Refused = 5,
}

impl From<Query> for RValue {
    /// Converts a Query to an RValue.
    fn from(query: Query) -> Self {
        match query {
            Query::Success => Self::Zero,
            Query::FormatError => Self::One,
            Query::ServerFailure => Self::Two,
            Query::NameError => Self::Three,
            Query::UnsupportedRequest => Self::Four,
            Query::Refused => Self::Five,
        }
    }
}

impl From<Query> for RError {
    /// Converts a Query to an RError.
    fn from(query: Query) -> Self {
        let rvalue: RValue = query.into();
        rvalue.into()
    }
}

impl From<Query> for RCode {
    /// Converts a Query to an RCode.
    fn from(query: Query) -> Self {
        Self::Query(query)
    }
}

/// The valid response codes for a release.
#[derive(Debug, Clone, Copy)]
pub enum Release {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    Refused = 5,
    ActiveError = 6,
}

impl From<Release> for RValue {
    /// Maps a Release to an RValue.
    fn from(release: Release) -> Self {
        match release {
            Release::Success => Self::Zero,
            Release::FormatError => Self::One,
            Release::ServerFailure => Self::Two,
            Release::Refused => Self::Five,
            Release::ActiveError => Self::Six,
        }
    }
}

impl From<Release> for RError {
    /// Maps a Release to an RError.
    fn from(release: Release) -> Self {
        let rvalue: RValue = release.into();
        rvalue.into()
    }
}

impl From<Release> for RCode {
    /// Maps a Release to an RCode.
    fn from(release: Release) -> Self {
        Self::Release(release)
    }
}

/// The valid response codes for a registration.
#[derive(Debug, Clone, Copy)]
pub enum Registration {
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    UnsupportedRequest = 4,
    Refused = 5,
    ActiveError = 6,
    ConflictError = 7,
}

impl From<Registration> for RValue {
    /// Maps a Registration to an RValue.
    fn from(registration: Registration) -> Self {
        match registration {
            Registration::Success => Self::Zero,
            Registration::FormatError => Self::One,
            Registration::ServerFailure => Self::Two,
            Registration::UnsupportedRequest => Self::Four,
            Registration::Refused => Self::Five,
            Registration::ActiveError => Self::Six,
            Registration::ConflictError => Self::Seven,
        }
    }
}

impl From<Registration> for RError {
    /// Maps a Registration to an RError.
    fn from(registration: Registration) -> Self {
        let rvalue: RValue = registration.into();
        rvalue.into()
    }
}

impl From<Registration> for RCode {
    /// Maps a Registration to an RCode.
    fn from(registration: Registration) -> Self {
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
            RValue::Zero => Self::Query(Query::Success),
            RValue::One => Self::Query(Query::FormatError),
            RValue::Two => Self::Query(Query::ServerFailure),
            RValue::Three => Self::Query(Query::NameError),
            RValue::Four => Self::Query(Query::UnsupportedRequest),
            RValue::Five => Self::Query(Query::Refused),
            r => Self::Custom(r),
        }
    }

    /// Maps a response code to a release response code.
    pub fn release(code: RValue) -> Self {
        match code {
            RValue::Zero => Self::Release(Release::Success),
            RValue::One => Self::Release(Release::FormatError),
            RValue::Two => Self::Release(Release::ServerFailure),
            RValue::Five => Self::Release(Release::Refused),
            RValue::Six => Self::Release(Release::ActiveError),
            r => Self::Custom(r),
        }
    }

    /// Maps a response code to a registration response code.
    pub fn registration(code: RValue) -> Self {
        match code {
            RValue::Zero => Self::Registration(Registration::Success),
            RValue::One => Self::Registration(Registration::FormatError),
            RValue::Two => Self::Registration(Registration::ServerFailure),
            RValue::Four => Self::Registration(Registration::UnsupportedRequest),
            RValue::Five => Self::Registration(Registration::Refused),
            RValue::Six => Self::Registration(Registration::ActiveError),
            RValue::Seven => Self::Registration(Registration::ConflictError),
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
