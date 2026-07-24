//! `#[view(const)]` asserts const accessors, so a view whose raw type is invisible
//! to the const dispatch (no closure annotations, no `raw = <ty>`) must be a clear
//! error — not the quiet fallback to the runtime closure-call form.

#[bnb::bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct Lich {
    header: bnb::u3,
    #[view(
        bits = 2,
        const,
        read = |raw, _s| raw,
        write = |v| v
    )]
    kind: bnb::u2,
    pad: bnb::u3,
}

fn main() {}
