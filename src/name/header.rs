use binrw::{binrw, io::Cursor, BinRead, BinWrite};
use derive_builder::Builder;
use modular_bitfield::prelude::*;

use crate::name::codes::RCode;
use crate::name::opcode::OpCode;
use crate::name::question::Question;
use crate::name::resource::Resource;
use crate::name::State;

/// Header for a NetBIOS Name Service (NBNS) packet.
#[binrw]
#[brw(big)]
#[derive(Builder)]
// TODO: Come up with a good convention for building unchecked? build
// #[builder(build_fn(skip))]
pub struct Header {
    /// Ensure the NBT name header follows certain soundness checks. Defaults to `true`.
    ///
    /// - When set to `true`, the builder will ensure the header follows soundness checks defined by the RFC.
    /// - When set to `false`, the builder will not ensure the header follows these checks and may result in undefined behavior when being sent/parsed.
    #[brw(ignore)]
    #[builder(default)]
    check_soundness: bool,

    /// The transaction ID of the request/response. The builder requires this field to be set.
    pub transaction_id: u16,

    /// Private field used for calculating the opcode, flags, and rcode.
    #[builder(setter(skip))]
    #[bw(calc = State::new().with_opcode(self.opcode).with_flags(self.flags).with_rcode(self.rcode.into()))]
    state: State,

    /// The operation being performed. The builder requires this field to be set.
    #[bw(ignore)]
    #[br(calc = state.opcode())]
    pub opcode: OpCode,

    /// The flags for the request/response. The builder requires this field to be set.
    #[bw(ignore)]
    #[br(calc = state.flags())]
    pub flags: Flags,

    /// The result of the request/response. The builder requires this field to be set.
    #[bw(ignore)]
    #[br(calc = RCode::from_state(state))]
    pub rcode: RCode,

    /// The number of questions in the request/response. Defaults to `0` if not set by builder.
    #[builder(default)]
    pub questions: u16,

    /// The number of answers in the request/response. Defaults to `0` if not set by builder.
    #[builder(default)]
    pub answers: u16,

    /// The number of authority records in the request/response. Defaults to `0` if not set by builder.
    #[builder(default)]
    pub authorities: u16,

    /// The number of additional records in the request/response. Defaults to `0` if not set by builder.
    #[builder(default)]
    pub additional: u16,

    /// The questions in the request/response.
    #[br(count = questions)]
    #[builder(default)]
    pub questions_entries: Vec<Question>,

    /// The answers in the request/response.
    #[br(count = answers)]
    #[builder(default)]
    pub answers_records: Vec<Resource>,

    /// The authority records in the request/response.
    #[br(count = authorities)]
    #[builder(default)]
    pub authorities_records: Vec<Resource>,

    /// The additional records in the request/response.
    #[br(count = additional)]
    #[builder(default)]
    pub additional_records: Vec<Resource>,
}

impl Header {
    /// Convert the header to a byte vector.
    ///
    /// # Example
    /// ```
    /// # use nbt::name::{HeaderBuilder, OpCode, Op, Query, Flags};
    /// let header = HeaderBuilder::default()
    ///     .transaction_id(0x0001)
    ///     .opcode(OpCode::new().with_op(Op::Query).with_response(false))
    ///     .flags(Flags::new())
    ///     .rcode(Query::Success.into())
    ///     .build().unwrap();
    /// let bytes = header.as_bytes();
    /// ```
    pub fn as_bytes(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::with_capacity(12));
        self.write(&mut buffer).unwrap();
        buffer.into_inner()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buffer = Cursor::new(bytes);
        Self::read(&mut buffer).unwrap()
    }
}

/// Available flags for a NetBIOS Name Service (NBNS) packet.
///
/// The flags are defined as follows:
/// ```text
///   0   1   2   3   4   5   6
/// +---+---+---+---+---+---+---+
/// |AA |TC |RD |RA | 0 | 0 | B |
/// +---+---+---+---+---+---+---+
/// ```
///
///
/// |Symbol| Description|
/// |------|------------|
/// |AA    | **Authoritative Answer:** <br> Must be zero (0) if `R` flag of `OPCODE` is zero(0). <br><br> If `R` flag is one (1) then if `AA` is one (1) then the node responding is an authority for the domain name. <br><br> End nodes responding to queries always set this bit in responses. |
/// |TC    | **Truncated:** <br> Set if this message was truncated because the datagram carrying it would be greater than 576 bytes in length. Use TCP to get the information from the NetBIOS Name Server.|
/// |RD    | **Recursion Desired:** <br> May only be set on a request to a NetBIOS Name Server. <br><br> The NBNS will copy its state into the response packet. <br><br> If one (1) the NBNS will iterate on the query, registration, or release.|
/// |RA    | **Recursion Available:** <br> Only valid in responses from a NetBIOS Name Server -- must be zero in all other responses. <br><br> If one (1) then the NBNS supports recursive query, registration, and release. <br><br> If zero (0) then the end-node must iterate for query and challenge for registration.|
/// |B     | **Broadcast:** <br> = 1: packet was broadcast or multicast <br> = 0: unicast|
/// |0     | **Reserved:** <br> These bits are not utilized by the RFC. The library however does expose these bits to be manipulated.|
#[bitfield(filled = false)]
#[derive(BitfieldSpecifier, Debug, Clone, Copy)]
pub struct Flags {
    /// Indicates if the response is authoritative.
    pub authoritative: bool,

    /// Indicates if the response is truncated.
    pub truncated: bool,

    /// Indicates if the response desires recursion.
    pub recursion_desired: bool,

    /// Indicates if the response has recursion available.
    pub recursion_available: bool,

    /// The field is reserved but can be set by the user to other values.
    pub reserved: B2,

    /// Indicates if the response is broadcast.
    pub broadcast: bool,
}

#[cfg(test)]
mod unit {
    use super::*;
    use binrw::{io::Cursor, BinWrite};
    use modular_bitfield::prelude::B2;

    use crate::name::{Op, OpCode, QueryCode};

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
            .transaction_id(0)
            .opcode(OpCode::new().with_op(Op::Registration))
            .flags(HeaderFlags::new().with_authoritative(true))
            .rcode(QueryCode::NameError.into())
            .questions(0x0001)
            .build()
            .unwrap();

        let mut buffer = Cursor::new(Vec::new());
        header.write(&mut buffer).unwrap();
        println!("{:?}", buffer.into_inner());

        println!("{:?}", header.opcode.op());
    }
}
