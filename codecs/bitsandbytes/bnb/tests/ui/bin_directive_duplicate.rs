//! A field directive given twice with different values used to keep one of them silently.
//! (A restatement of the same value is the directive once, and `assert` may repeat: its
//! guards run in order.)
use bnb::bin;

#[bin]
struct SameAttr {
    n: u8,
    m: u8,
    #[br(count = n, count = m)]
    items: Vec<u8>,
}

#[bin]
struct TwoAttrs {
    #[br(temp)]
    #[bw(calc = 1)]
    #[bw(calc = 2)]
    len: u8,
}

#[bin]
struct TwoReserved {
    #[reserved]
    #[reserved_with(0xFFu8)]
    pad: u8,
}

fn main() {}
