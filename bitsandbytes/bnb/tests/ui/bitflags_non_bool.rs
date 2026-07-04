//! Every `#[bitflags]` field is one flag and must be `bool`.

use bnb::bitflags;

#[bitflags(u8)]
#[derive(Clone, Copy)]
struct F {
    a: bool,
    b: u8, // not a `bool` — each flag is a single bit
}

fn main() {}
