//! A variant with `tag = …` needs the enum to declare the selector via
//! `#[bin(tag = <ctx-param>)]` (the off-wire dispatch source).
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(tag = 1)]
    A(u8),
}

fn main() {}
