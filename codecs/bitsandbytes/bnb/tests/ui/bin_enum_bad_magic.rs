//! A `magic` must be a byte-oriented literal with an unambiguous width: a byte string or
//! a width-suffixed unsigned integer. An unsuffixed integer is rejected.
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(magic = 1)]
    A(u8),
}

fn main() {}
