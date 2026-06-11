use bits::{wire, u4};

#[wire(big, group(a, b => u8))]
struct X {
    #[update(self.x)]
    a: u4,
    b: u4,
    x: u8,
}

fn main() {}
