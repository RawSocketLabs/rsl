//! `codec` (a fn pair owns the wire form) and struct-level wire mapping (a wire *type*
//! owns it) are two answers to the same question — combining them is contradictory.
use bnb::bin;

#[bin(codec = bnb::codecs::leb128, wire = u64)]
struct Varint(u64);

fn main() {}
