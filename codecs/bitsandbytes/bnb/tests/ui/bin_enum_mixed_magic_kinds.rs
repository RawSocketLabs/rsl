//! An enum's dispatching magics are read as one type, so a byte string and an integer of
//! the same width cannot be mixed. (This used to panic the macro.)
use bnb::bin;

#[bin(big)]
enum Mixed {
    #[bin(magic = 1u8)]
    Int(u8),
    #[bin(magic = b"B")]
    Bytes(u8),
}

fn main() {}
