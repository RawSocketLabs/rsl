//! The prefix must be a `CountPrefix` type (u8..u128 or a `uN` alias) — a `String`
//! has no defined wire width or length conversion.
use bnb::bin;

#[bin]
struct Frame {
    tag: bnb::u4,
    #[brw(count_prefix = String)]
    items: Vec<u8>,
}

fn main() {}
