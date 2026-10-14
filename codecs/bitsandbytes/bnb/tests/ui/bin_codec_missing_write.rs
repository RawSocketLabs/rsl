//! The paren form may omit a direction's fn only when the struct is narrowed to the
//! other direction — a bidirectional newtype with no `write` cannot encode.
use bnb::bin;

#[bin(codec(parse = bnb::codecs::leb128::parse))]
struct Varint(u64);

fn main() {}
