//! `CountPrefix` and `codecs::leb128::Varint` are sealed — the impl sets are
//! crate-owned (all wire-integer widths are built in), so a downstream impl is a
//! compile error: the `Sealed` supertrait is unnameable outside `bnb`.
use bnb::codecs::{CountPrefix, leb128::Varint};

#[derive(Clone, Copy)]
struct MyPrefix(u8);

impl bnb::Bits for MyPrefix {
    const BITS: u32 = 8;
    fn into_bits(self) -> u128 {
        self.0 as u128
    }
    fn from_bits(raw: u128) -> Self {
        MyPrefix(raw as u8)
    }
}

impl CountPrefix for MyPrefix {
    fn try_from_len(_: usize) -> Result<Self, bnb::WidthError> {
        unimplemented!()
    }
    fn to_count(self) -> usize {
        unimplemented!()
    }
}

impl Varint for MyPrefix {}

fn main() {}
