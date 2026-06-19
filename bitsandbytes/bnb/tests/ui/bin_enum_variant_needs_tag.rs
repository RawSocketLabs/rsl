//! A variant with neither `tag`, `magic`, nor `#[catch_all]` is a typed fallback —
//! not yet implemented in this Phase 1 step.
use bnb::bin;

#[bin(big)]
enum E {
    #[bin(magic = 1u8)]
    A(u8),
    B(u8), // no magic / tag — a fallback
}

fn main() {}
