//! Option (a): a `#[bin]` enum's "nothing matched" tail is a typed fallback OR a
//! `#[catch_all]`, not both.
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(magic = 1u8)]
    A(u8),
    B(u8), // a typed fallback...
    #[catch_all]
    C { x: u8 }, // ...and a catch_all — at most one tail
}

fn main() {}
