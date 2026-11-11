//! A `#[bitfield]` field whose generated accessors would reuse a name the macro already
//! generates is an error at the field, not a duplicate-definition error from inside the
//! generated impl.
use bnb::{BitsBuilder, bitfield};

// `new` is the generated all-zero constructor.
#[bitfield(u8)]
#[derive(Clone, Copy)]
struct Fixed {
    new: bnb::u4,
    rest: bnb::u4,
}

// The getter of `with_low` is the `with_` setter of `low`.
#[bitfield(u8)]
#[derive(Clone, Copy)]
struct Sibling {
    low: bnb::u4,
    with_low: bnb::u4,
}

// With the builder, `builder()` is generated too.
#[bitfield(u8)]
#[derive(BitsBuilder, Clone, Copy)]
struct WithBuilder {
    builder: bnb::u4,
    rest: bnb::u4,
}

fn main() {}
