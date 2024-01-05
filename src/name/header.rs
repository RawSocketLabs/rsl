use binrw::{binrw, BinRead, BinWrite};
use derive_builder::Builder;
use modular_bitfield::prelude::*;
use thiserror::Error;

/// Header for a NetBIOS Name Service (NBNS) packet.
#[binrw]
#[brw(big)]
#[derive(Builder)]
pub struct Header {
    pub transacition_id: u16,
    #[builder(setter(skip))]
    #[bw(calc = State::new().with_opcode(self.opcode).with_flags(self.flags).with_rcode(self.rcode.into()))]
    state: State,
    #[bw(ignore)]
    #[br(calc = state.opcode())]
    pub opcode: OpCode,
    #[bw(ignore)]
    #[br(calc = state.flags())]
    pub flags: Flags,
    #[bw(ignore)]
    #[br(calc = RCode::from_state(state))]
    pub rcode: RCode,
    pub questions: u16,
    pub answers: u16,
    pub authorities: u16,
    pub additional: u16,
}

/// Bitfield representation of opcode, flags, and rcode.
///
/// This private struct is utilized because bitfields take up space
/// in increments of 8 bits. By collapsing down the 3 fields we essentially get
/// a 16 bit bitfield.
///
/// The structure is used for effectively reading and writing and does bloat the size
/// of the header by 2 bytes, however for ease of use this appears worthwhile.
///
/// The struct is only used internally and is not exposed to the user.
#[bitfield]
#[derive(BinWrite, BinRead, Clone, Copy, Debug, Default)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
struct State {
    #[bits = 5]
    opcode: OpCode,
    #[bits = 7]
    flags: Flags,
    #[bits = 4]
    rcode: RValue,
}

/// Encodes the operation being performed, and indicates whether the message
/// is a request or a response.
#[bitfield(filled = false)]
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
pub struct OpCode {
    pub response: bool,
    pub op: Op,
}

#[derive(Debug, Clone, Copy, PartialEq, BitfieldSpecifier)]
#[bits = 4]
pub enum Op {
    Query = 0,
    Registration = 5,
    Release = 6,
    Wack = 7,
    Refresh = 8,

    // The following are not part of the RFC, but are available to the consumer
    // of this library for custom use.
    Custom1 = 1,
    Custom2 = 2,
    Custom3 = 3,
    Custom4 = 4,
    Custom9 = 9,
    Custom10 = 10,
    Custom11 = 11,
    Custom12 = 12,
    Custom13 = 13,
    Custom14 = 14,
    Custom15 = 15,
}

/// Available flags for a NetBIOS Name Service (NBNS) packet.
#[bitfield(filled = false)]
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
pub struct Flags {
    /// Indicates if the response is authoritative.
    pub authoritative: bool,
    pub truncated: bool,
    pub recursion_desired: bool,
    pub recursion_available: bool,
    pub reserved: B2,
    pub broadcast: bool,
}

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

/// The underlying response value. This value can be mapped to an RCode which is aware of the
/// operation being performed.
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

impl From<RValue> for u8 {
    fn from(r: RValue) -> Self {
        r.into()
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
    fn from_state(state: State) -> Self {
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

#[derive(Error, Debug, Clone, Copy)]
pub enum RError {
    #[error("Success")]
    Success,
    #[error("Format: Request was invalidly formatted.")]
    FormatError,
    #[error("Server Failure: Problem with NBNS, cannot process name.")]
    ServerFailure,
    #[error("Name: The name requested does not exist.")]
    NameError,
    #[error("Unsupported Request: Allowable only for challenging NBNS when gets an Update type registration request.")]
    UnsupportedRequest,
    #[error("Refused: For policy reasons server will not register this name from this host.")]
    Refused,
    #[error("Active: Name is owned by another node.")]
    ActiveError,
    #[error("Conflict: Name is owned by another node.")]
    ConflictError,
    #[error("Unknown: Unknown error code {0}")]
    Unknown(u8),
}

impl From<RValue> for RError {
    /// Converts an RValue to an RError.
    fn from(rvalue: RValue) -> Self {
        match rvalue {
            RValue::Zero => Self::Success,
            RValue::One => Self::FormatError,
            RValue::Two => Self::ServerFailure,
            RValue::Three => Self::NameError,
            RValue::Four => Self::UnsupportedRequest,
            RValue::Five => Self::Refused,
            RValue::Six => Self::ActiveError,
            RValue::Seven => Self::ConflictError,
            r => Self::Unknown(r.into()),
        }
    }
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
        query.into()
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
        release.into()
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
        registration.into()
    }
}

impl From<Registration> for RCode {
    fn from(registration: Registration) -> Self {
        Self::Registration(registration)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use binrw::io::Cursor;

    #[test]
    fn opcodes() {
        let mut opcode = OpCode::new();

        opcode.set_op(Op::Custom3);

        assert_eq!(opcode.op(), Op::Custom3);
    }

    #[test]
    fn flags() {
        let mut flags = Flags::new();

        flags.set_authoritative(true);
        flags.set_truncated(true);
        flags.set_recursion_desired(true);
        flags.set_recursion_available(true);
        flags.set_broadcast(true);
        flags.set_reserved(B2::from_bytes(0x03).unwrap());

        println!("{:?}", flags);
    }

    #[test]
    fn builder() {
        let header = HeaderBuilder::default()
            .transacition_id(0)
            .opcode(OpCode::new().with_op(Op::Registration))
            .flags(Flags::new().with_authoritative(true))
            .rcode(Query::NameError.into())
            .questions(0x0001)
            .answers(0x0000)
            .authorities(0x0000)
            .additional(0x0000)
            .build()
            .unwrap();

        let mut buffer = Cursor::new(Vec::new());
        header.write(&mut buffer).unwrap();
        println!("{:?}", buffer.into_inner());

        println!("{:?}", header.opcode.op());
    }
}
