//! `count_prefix` reads a prefix and sizes the elements it counts — that only makes
//! sense on a `Vec<_>` field. Any other type is an error.
use bnb::bin;

#[bin]
struct Frame {
    tag: bnb::u4,
    #[brw(count_prefix = u16)]
    value: u32,
}

fn main() {}
