//! A `#[bitfield]` must use one width style throughout: either inferred/`#[bits(N)]`
//! widths (auto-placed) or `#[bits(A..=B)]` ranges (manual), never a mix.

use bnb::{bitfield, u4};

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy)]
struct Mixed {
    #[bits(0..=7)]
    a: u8, // a manual range...
    b: u4, // ...mixed with an inferred width
}

fn main() {}
