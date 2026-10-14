use bnb::BitEnum;

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(bnb::u2)]
enum OutOfWidth {
    A = 0,
    B = 1,
    C = 2,
    D = 9,
}

fn main() {}
