use binrw::{BinRead, BinWrite};
use modular_bitfield_msb::prelude::*;

use crate::name::codes::{OpCode, RValue};
use crate::name::header::Flags;

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
#[derive(BinWrite, BinRead, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
pub(crate) struct State {
    #[bits = 5]
    pub(crate) opcode: OpCode,

    #[bits = 7]
    pub(crate) flags: Flags,

    #[bits = 4]
    pub(crate) rcode: RValue,
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::name::codes::Op;

    #[test]
    fn test_state() {
        let state_bin = (0b0100_0011_0010_1100 as u16).to_be_bytes();
        let state_one = State::from_bytes([0x01, 0x10]);
        let state_two = State::from_bytes(state_bin);

        println!("{:#?}", state_one);
        println!("{:#?}", state_two);

        let state = State::new()
            .with_opcode(OpCode::new().with_response(false).with_op(Op::Query))
            .with_flags(
                Flags::new()
                    .with_recursion_desired(true)
                    .with_broadcast(true),
            )
            .with_rcode(RValue::Zero);

        println!("{:#?}", state);

        println!("{:#?}", state.into_bytes());
    }
}
