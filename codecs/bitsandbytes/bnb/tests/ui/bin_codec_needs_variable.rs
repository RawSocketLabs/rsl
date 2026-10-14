//! A codec newtype is variable-length (no `FixedBitLen`), so a parent that would
//! otherwise be fixed-width can't sum its bits: the fix is `#[brw(variable)]` on the
//! field (or, for a genuinely fixed codec, a manual one-line `FixedBitLen` impl).
use bnb::bin;

#[bin(codec = bnb::codecs::leb128)]
#[derive(Debug, PartialEq)]
struct Varint(u64);

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Frame {
    kind: u8,
    length: Varint, // ← missing #[brw(variable)]
    crc: u16,
}

fn main() {}
