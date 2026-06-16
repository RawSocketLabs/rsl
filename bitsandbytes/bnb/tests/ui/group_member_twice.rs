use bnb::{wire, u4};

#[wire(big, group(a, b => u8), group(b, c => u8))]
struct X {
    a: u4,
    b: u4,
    c: u4,
}

fn main() {}
