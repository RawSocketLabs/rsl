//! A decode-only mapped struct decodes in its wire type's byte and bit order, so the wire
//! type has to declare one: any `#[bin]` type does, and a hand-written decoder does through
//! `BitEncode::LAYOUT`. One error, at the wire type, in those terms.
use bnb::{BitDecode, BitError, Source, bin};

struct Handwritten(u16);

impl BitDecode for Handwritten {
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        Ok(Handwritten(r.read()?))
    }
}

#[bin(read_only, wire = Handwritten)]
struct Mapped(u16);

impl From<Handwritten> for Mapped {
    fn from(wire: Handwritten) -> Self {
        Mapped(wire.0)
    }
}

fn main() {}
