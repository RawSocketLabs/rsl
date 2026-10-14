//! `count_prefix` injects a hidden length field, which needs `#[bin]`'s struct
//! ownership — the bare derives can't re-emit the item, so they reject the directive.
use bnb::{BitDecode, BitEncode};

#[derive(BitDecode, BitEncode)]
struct Frame {
    tag: bnb::u4,
    #[brw(count_prefix = u16)]
    items: Vec<u8>,
}

fn main() {}
