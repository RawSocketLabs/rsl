//! A primitive `type` alias as a `#[bitfield]` field type is not recognized by the
//! const dispatch — the macro sees only the alias token, so it emits the inherent
//! const-pair call that primitives don't have. Documented failure mode: use the bare
//! primitive name (`u8`), or wrap the type and implement it via `bnb::impl_bits!`.
use bnb::bitfield;

type Byte = u8;

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy)]
struct Framed {
    tag: Byte,
    len: u8,
}

fn main() {}
