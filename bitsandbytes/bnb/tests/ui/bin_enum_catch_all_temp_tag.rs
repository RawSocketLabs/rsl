//! A `#[catch_all]` variant's first field holds the captured discriminant, so it can't
//! be `#[br(temp)]` (the discriminant would be lost and couldn't round-trip).
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(magic = 1u8)]
    A(u8),
    #[catch_all]
    Other {
        #[br(temp)]
        #[bw(calc = 0u8)]
        magic: u8,
        rest: u8,
    },
}

fn main() {}
