use bnb::bitfield;

#[bitfield(u8)]
struct OutOfBacking {
    #[bits(0..=8)]
    value: u16,
}

fn main() {}
