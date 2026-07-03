//! `count_prefix` generates the length field and the count expression itself, so a
//! directive that reads, stores, or replaces that machinery on the same field conflicts.
use bnb::bin;

#[bin]
struct Frame {
    tag: bnb::u4,
    n: u8,
    #[brw(count_prefix = u16)]
    #[br(count = n)]
    items: Vec<u8>,
}

fn main() {}
