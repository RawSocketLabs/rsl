//! A reversed `#[bits(high..=low)]` range must be a clear, spanned error — not an
//! opaque const-eval "subtract with overflow" panic.
use bnb::bitfield;

#[bitfield(u16)]
struct Reversed {
    #[bits(15..=11)] // reversed: ranges are written low..=high (i.e. 11..=15)
    opcode: bnb::u5,
    #[bits(0..=10)]
    rest: bnb::u11,
}

fn main() {}
