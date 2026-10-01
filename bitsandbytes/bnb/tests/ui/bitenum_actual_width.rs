#![allow(non_camel_case_types)]
use bnb::BitEnum;
type u2 = bnb::u3;

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u2)]
enum WrongWidthName {
    A,
    B,
    C,
    D,
}

fn main() {}
