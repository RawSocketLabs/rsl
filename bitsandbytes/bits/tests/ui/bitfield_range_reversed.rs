//! A reversed `#[bits(high..=low)]` range must be a clear, spanned error — not an
//! opaque const-eval "subtract with overflow" panic.
use bits::bitfield;

#[bitfield(u16)]
struct Reversed {
    #[bits(15..=11)] // reversed: ranges are written low..=high (i.e. 11..=15)
    opcode: bits::u5,
    #[bits(0..=10)]
    rest: bits::u11,
}

fn main() {}
