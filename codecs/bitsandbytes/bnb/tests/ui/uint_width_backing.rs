use bnb::UInt;

const INVALID: UInt<u8, 16> = UInt::<u8, 16>::MIN;

fn main() {
    let _ = INVALID;
}
