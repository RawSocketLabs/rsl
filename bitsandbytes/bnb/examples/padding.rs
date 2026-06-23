//! **padding** — a second `pad`/`align` orchestration: a sub-byte header field, then
//! `align_before` to realign to a byte boundary before the body, and `pad_after` for a
//! fixed-size trailing reserved region. (Complements `alignment`, which showed `pad_before` +
//! `align_after`; the four directives are `pad_before`/`pad_after`/`align_before`/`align_after`.)
//!
//! Run with: `cargo run -p bitsandbytes --example padding`

use bnb::prelude::*; // the `3.bytes()` amount helper
use bnb::{bin, u3};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Record {
    kind: u3, // 3 bits -> the cursor is now mid-byte
    #[br(align_before)] // realign to the next byte boundary before `length`
    length: u16,
    flags: u8,
    #[br(pad_after = 3.bytes())] // 3 reserved trailing bytes -> a fixed 8-byte footprint
    crc: u8,
}

fn main() {
    let r = Record {
        kind: u3::new(0b101),
        length: 0x1234,
        flags: 0xAB,
        crc: 0xC7,
    };
    let bytes = r.to_bytes().unwrap();
    // kind|align-pad = byte0, length = byte1..3, flags = byte3, crc = byte4, pad = byte5..8.
    println!("encoded: {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(bytes.len(), 8);
    assert_eq!(Record::decode_exact(&bytes).unwrap(), r);
    println!("{r:#?}");
    println!("all checks passed");
}
