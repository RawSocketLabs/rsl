//! A builder field's setter is named after the field, so a field named `build` would
//! collide with the builder's own `build()`.
use bnb::{BitsBuilder, bin};

#[derive(BitsBuilder)]
struct Plain {
    build: u8,
}

#[bin(big)]
struct Message {
    tag: u8,
    build: u8,
}

fn main() {}
