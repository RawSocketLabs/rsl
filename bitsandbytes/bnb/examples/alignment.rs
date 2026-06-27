//! **alignment** — `pad`/`align` positioning: skip reserved bits between fields and align to a
//! byte boundary, with typed amounts from `bnb::prelude` (`4.bits()`, `1.bytes()`). These are
//! the *forward* positioning directives; backward seeks need a `SeekSource` (see `peek` /
//! `archive`).
//!
//! Run with: `cargo run -p bitsandbytes --example alignment`

use bnb::prelude::*; // the `4.bits()` / `1.bytes()` amount helpers
use bnb::{bin, u4};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Header {
    kind: u4,

    #[br(pad_before = 4.bits())] // 4 reserved bits between `kind` and `flags`
    flags: u8,

    #[br(pad_before = 1.bytes())] // a full reserved byte before `value`
    value: u16,

    #[br(align_after)] // pad to the next byte boundary after this 4-bit field
    trailer: u4,

    extra: u8,
}

fn main() {
    let h = Header {
        kind: u4::new(0x5),
        flags: 0xAB,
        value: 0x1234,
        trailer: u4::new(0x7),
        extra: 128,
    };
    let bytes = h.to_bytes().unwrap();
    // kind|pad = byte0, flags = byte1, pad = byte2, value = byte3..5, trailer|pad = byte5, extra = byte6.
    println!("encoded: {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
    println!("{h:#?}");
    println!("all checks passed");
}
