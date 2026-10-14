//! `#[view(const)]` (assert const accessors) and `dynamic` (opt out of them)
//! contradict each other — rejected with a clear error naming both.

#[bnb::bitfield(u8, bits = msb)]
#[derive(Clone, Copy)]
struct Lich {
    header: bnb::u3,
    #[view(
        bits = 2,
        const,
        dynamic,
        read = |raw: bnb::u2, _s: &Self| raw,
        write = |v: bnb::u2| v
    )]
    kind: bnb::u2,
    pad: bnb::u3,
}

fn main() {}
