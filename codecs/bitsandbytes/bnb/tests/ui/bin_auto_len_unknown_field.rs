//! `auto_len` naming a field that doesn't exist is a clear error, not a silently-ignored
//! spec that leaves the nested `WireLen` unresolved.
use bnb::{WireLen, bin};

#[bin(big)]
#[derive(Clone)]
struct Hdr {
    n: WireLen<u16>,
}

#[bin(big, auto_len(headr.n = count(items)))]
struct Msg {
    hdr: Hdr,
    #[br(count = hdr.n.to_count())]
    items: Vec<u8>,
}

fn main() {}
