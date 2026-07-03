//! The whole-message options don't apply to a codec newtype — the codec functions own
//! the framing (put a magic inside the fns, or use a full `#[bin]` struct).
use bnb::bin;

#[bin(codec = bnb::codecs::leb128, magic = 0xCAFEu16)]
struct Varint(u64);

fn main() {}
