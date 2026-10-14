//! `codec` is a *per-type* codec — it needs a newtype (a single-field tuple struct),
//! not a named-field struct: there is exactly one inner value the fn pair carries.
use bnb::bin;

#[bin(codec = bnb::codecs::leb128)]
struct Frame {
    value: u64,
}

fn main() {}
