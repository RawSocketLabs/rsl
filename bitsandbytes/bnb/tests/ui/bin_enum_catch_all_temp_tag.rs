//! A `#[catch_all]` variant's first field holds the captured tag, so it can't be temp
//! (the tag would be lost and couldn't round-trip on encode).
use bnb::bin;

#[bin(big, tag = u8)]
enum E {
    #[bin(tag = 1)]
    A(u8),
    #[catch_all]
    Other {
        #[br(temp)]
        #[bw(calc = 0u8)]
        tag: u8,
        rest: u8,
    },
}

fn main() {}
