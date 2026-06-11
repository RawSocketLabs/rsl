use bits::{wire, u4};

#[wire(big, group(a, b => u8))]
struct X {
    #[br(temp)]
    a: u4,
    b: u4,
}

fn main() {}
