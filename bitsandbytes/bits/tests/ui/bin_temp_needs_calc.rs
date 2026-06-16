//! A `#[br(temp)]` field is read into a local but not stored, so on the write side
//! it has no `self` value — it needs `#[bw(calc = …)]`. Omitting it is an error.
use bits::bin;

#[bin]
struct Frame {
    tag: bits::u4,
    #[br(temp)]
    count: u16,
    #[br(count = count)]
    data: Vec<u8>,
}

fn main() {}
