use bnb::{wire, u4};

#[wire(big, group(a, c => u8))]
struct X {
    a: u4,
    b: u4,
    c: u4,
    d: u4,
}

fn main() {}
