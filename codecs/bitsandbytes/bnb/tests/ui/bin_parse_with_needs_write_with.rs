//! A `#[br(parse_with = …)]` field needs the inverse `#[bw(write_with = …)]` to be
//! encodable — a read-only custom codec can't round-trip.

use bnb::{BitError, Source, bin};

fn rd<S: Source>(_r: &mut S) -> Result<u8, BitError> {
    Ok(0)
}

#[bin(big)]
struct X {
    #[br(parse_with = rd)]
    v: u8,
}

fn main() {}
