//! `#[bin]`'s `read_only` and `write_only` are mutually exclusive.

use bnb::bin;

#[bin(big, read_only, write_only)]
struct X {
    a: u8,
}

fn main() {}
