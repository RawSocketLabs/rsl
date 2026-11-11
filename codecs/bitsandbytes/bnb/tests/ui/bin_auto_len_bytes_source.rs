//! `auto_len = bytes(x)` replays `x`'s own write where the length is written, so `x` must be
//! a field of the message.
use bnb::bin;

#[bin(big)]
struct Unknown {
    #[bw(auto_len = bytes(bodi))]
    len: bnb::WireLen<u8>,
    body: u32,
}

fn main() {}
