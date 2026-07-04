//! `auto_len` targeting the same `field.nested` twice is a compile error — the second
//! resolve would be a silent no-op.
use bnb::{WireLen, bin};

#[bin(big)]
#[derive(Clone)]
struct Hdr {
    n: WireLen<u16>,
}

#[bin(big, auto_len(hdr.n = count(a), hdr.n = count(b)))]
struct Msg {
    hdr: Hdr,
    #[br(count = hdr.n.to_count())]
    a: Vec<u8>,
    #[br(count = hdr.n.to_count())]
    b: Vec<u8>,
}

fn main() {}
