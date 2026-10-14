//! `codec` carries exactly one inner value — a multi-field tuple struct is rejected.
use bnb::bin;

#[bin(codec = bnb::codecs::leb128)]
struct Pair(u64, u64);

fn main() {}
