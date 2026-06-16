//! `#[bin(forward_only)]` pins a `Source`-only bound, so a seek directive
//! (`restore_position`) is a compile error.
use bits::bin;

#[bin(forward_only)]
struct Frame {
    a: bits::u4,
    #[br(restore_position)]
    peek: u8,
    value: u16,
}

fn main() {}
