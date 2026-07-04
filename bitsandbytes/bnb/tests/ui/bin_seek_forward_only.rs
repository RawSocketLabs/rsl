//! `#[bin(forward_only)]` pins a `Source`-only bound, so `#[br(seek = …)]` (which
//! needs to seek) is a compile error — symmetric with `restore_position`.
use bnb::bin;

#[bin(forward_only)]
struct Frame {
    ptr: u8,
    #[br(seek = 16u32)]
    target: u8,
}

fn main() {}
