//! A read-side `#[br(map = …)]` transforms the wire repr into a non-`Bits` field
//! type, so encoding needs the inverse `#[bw(map = …)]`. Omitting it is an error.
use bits::bin;

#[derive(Clone)]
struct Wrapped(u16);

#[bin]
struct Frame {
    tag: bits::u4,
    #[br(map = |raw: u16| Wrapped(raw))]
    value: Wrapped,
}

fn main() {}
