//! A `#[derive(BitEnum)]` may have at most one `#[catch_all]` variant.

use bnb::{BitEnum, u4};

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u4)]
enum E {
    A,
    #[catch_all]
    X(u4),
    #[catch_all]
    Y(u4),
}

fn main() {}
