//! A `#[flag(N)]` whose bit is out of range for the backing integer is a clear error, not
//! a const shift-overflow deep in generated code.
use bnb::bitflags;

#[bitflags(u8)]
struct Flags {
    a: bool,
    #[flag(200)]
    too_high: bool,
}

fn main() {}
