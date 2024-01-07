use binrw::{binrw, io::Cursor, BinRead, BinWrite};
use derive_builder::Builder;

use crate::name::{Flags, OpCode, Question, RCode, Resource, State};

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
    pub transacition_id: u16,

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
    pub answers_records: Vec<Resource>,

    /// The authority records in the request/response.
    #[br(count = authorities)]
    pub authorities_records: Vec<Resource>,

    /// The additional records in the request/response.
    #[br(count = additional)]
    pub additional_records: Vec<Resource>,
}

impl Header {
    /// Convert the header to a byte vector.
    ///
    /// # Example
    /// ```
    /// # use nbt::name::{HeaderBuilder, OpCode, Op, Query, Flags};
    /// let header = HeaderBuilder::default()
    ///     .transacition_id(0x0001)
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

#[cfg(test)]
mod unit {
    use super::*;
    use binrw::{io::Cursor, BinWrite};

    use crate::name::{Op, OpCode, Query};

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
