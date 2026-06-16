//! A `count`-bearing (variable-length) message implements `BitDecode`/`BitEncode`
//! but NOT `FixedBitLen` — so asking for its const `BIT_LEN` is a compile error.
use bnb::{FixedBitLen, bin};

#[bin]
struct Msg {
    tag: bnb::u4,
    n: u8,
    #[br(count = n)]
    data: Vec<u8>,
}

fn main() {
    let _ = <Msg as FixedBitLen>::BIT_LEN;
}
