use binrw::{BinRead, BinWrite};
use modular_bitfield::prelude::*;

use crate::name::{Flags, OpCode, RValue};

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
pub(crate) struct State {
    #[bits = 5]
    pub(crate) opcode: OpCode,
    #[bits = 7]
    pub(crate) flags: Flags,
    #[bits = 4]
    pub(crate) rcode: RValue,
}
