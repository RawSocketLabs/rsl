use bnb::{wire, u4};

// a + b = 8 bits, but the backing is u16 (16 bits): the group does not fill the
// backing, so the layout would be ambiguous. This must be a compile error.
#[wire(big, group(a, b => u16))]
struct X {
    a: u4,
    b: u4,
}

fn main() {}
