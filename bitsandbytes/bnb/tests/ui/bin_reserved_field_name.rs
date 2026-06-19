//! A field named `r`/`w` collides with the codec's generated source/sink.
use bnb::bin;

#[bin(big)]
struct Frame {
    w: u8,
    h: u8,
}

fn main() {}
