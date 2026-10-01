use bnb::BitEnum;

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u128, closed)]
enum Overflow {
    Last = 340282366920938463463374607431768211455,
    TooFar,
}

fn main() {}
