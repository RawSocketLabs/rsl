//! A `#[bitfield]` whose declared fields are wider than the backing integer must be a
//! compile error — otherwise the generated accessors would silently truncate the
//! high fields on write. The width-fit assert is forced by a `const _`, so it fires
//! even though nothing references the type.

use bnb::{bitfield, u4};

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct TooWide {
    a: u4,
    b: u4,
    c: u4, // 4 + 4 + 4 = 12 bits do not fit in a u8 backing
}

fn main() {}
