//! A `#[derive(BitEnum)]` with no `#[catch_all]` whose variants do not cover its
//! width must be a compile error — decoding an unknown discriminant via the codec
//! (or a `#[bitfield]` getter) would panic. The diagnostic steers the author to
//! `#[catch_all]` (preserve unknowns, dual-use) or `#[bit_enum(.., closed)]` (assert
//! a closed set).

use bnb::BitEnum;

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u8)]
enum Status {
    Ok = 0,
    Err = 1,
}

fn main() {}
