use bits::{wire, u4};

#[wire(big, group(b, a => u8))]
struct X {
    a: u4,
    b: u4,
}

fn main() {}
