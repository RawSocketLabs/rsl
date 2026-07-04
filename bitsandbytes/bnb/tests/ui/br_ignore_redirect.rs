//! `ignore` applies to both directions, so it must be written `#[brw(ignore)]`.
//! `#[br(ignore)]` is rejected with a diagnostic pointing at the right spelling.

use bnb::bin;

#[bin(big)]
struct X {
    a: u8,
    #[br(ignore)]
    note: u32,
}

fn main() {}
