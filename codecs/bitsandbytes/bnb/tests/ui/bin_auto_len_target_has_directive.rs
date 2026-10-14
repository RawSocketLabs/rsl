//! A field targeted by `auto_len(...)` cannot also carry a codec directive — the
//! clone-and-fill write would silently drop it, desyncing encode from decode.
use bnb::{WireLen, bin};

#[bin(big)]
#[derive(Clone)]
struct Hdr {
    n: WireLen<u16>,
}

#[bin(big, auto_len(hdr.n = count(items)))]
struct Msg {
    #[bw(map = |h: &Hdr| h.clone())]
    hdr: Hdr,
    #[br(count = hdr.n.to_count())]
    items: Vec<u8>,
}

fn main() {}
