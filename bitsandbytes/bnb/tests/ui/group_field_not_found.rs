use bnb::{wire, u4};

#[wire(big, group(a, missing => u8))]
struct X {
    a: u4,
    b: u4,
}

fn main() {}
