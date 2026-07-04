//! `#[bw(auto_len = <expr>)]` must be `count(<field>)` or `bytes(<field>)`.
use bnb::{WireLen, bin};

#[bin(big)]
struct Msg {
    #[bw(auto_len = size(items))]
    n: WireLen<u16>,
    #[br(count = n.to_count())]
    items: Vec<u8>,
}

fn main() {}
