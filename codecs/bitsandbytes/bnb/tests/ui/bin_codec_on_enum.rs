//! `codec` applies to a newtype, not an enum — an enum's wire form is its dispatch.
use bnb::bin;

#[bin(codec = bnb::codecs::leb128)]
enum Message {
    A(u64),
    B(u64),
}

fn main() {}
