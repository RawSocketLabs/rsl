//! A `#[bits(N)]` slot wider than its field type's `Bits::BITS` must be a compile error:
//! the getter would hand the type bits it cannot hold (a `BitEnum` then panics on an
//! unknown discriminant; a primitive silently truncates).

use bnb::{bitfield, u2, BitEnum};

#[derive(BitEnum, Clone, Copy, Debug)]
#[bit_enum(u2)]
enum Mode {
    A,
    B,
    C,
    D,
}

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct Header {
    #[bits(3)]
    mode: Mode, // a 2-bit enum in a 3-bit slot
    rest: u2,
}

fn main() {}
