//! **reserved** — `#[reserved]` fields + the **verbatim vs canonical** encode model. A reserved
//! field is a normal *stored* field with a known spec value: the decoder captures the actual
//! wire bits (dual-use — observable and overridable), `to_bytes` re-emits them **verbatim**, and
//! `to_canonical_bytes` **normalizes** them to spec. The in-memory helpers (`is_canonical`,
//! `canonical_diff`, `to_canonical`) and the value-carried `encode_mode` round it out.
//!
//! Reserved-bearing types are builder/decode-constructed — a hidden `encode_mode` field means
//! there's no struct literal.
//!
//! Run with: `cargo run -p bitsandbytes --example reserved`

use bnb::prelude::*; // brings `EncodeExt::encode(writer)` into scope
use bnb::{EncodeMode, bin, u4};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    version: u4,
    #[reserved]
    rsv: u4, // spec value is 0
    payload: u16,
}

fn main() {
    // A peer left non-spec bits in the reserved nibble; the decoder captures them verbatim.
    let received = Frame::decode_exact(&[0x5F, 0x12, 0x34]).unwrap();
    println!("{received:#?}");
    assert_eq!(received.rsv, u4::new(0xF));

    // Verbatim re-emits exactly what's held...
    assert_eq!(received.to_bytes().unwrap(), [0x5F, 0x12, 0x34]);
    // ...canonical forces the reserved field back to its spec value.
    assert_eq!(received.to_canonical_bytes().unwrap(), [0x50, 0x12, 0x34]);

    // In-memory helpers: is it canonical, and which fields differ?
    println!("is_canonical = {}", received.is_canonical());
    println!("canonical_diff = {:?}", received.canonical_diff());
    assert!(!received.is_canonical());
    assert_eq!(received.canonical_diff(), vec!["rsv"]);

    // The mode rides on the value: set it to Canonical and the std `encode(writer)` follows.
    let mut normalized = received.clone();
    normalized.set_encode_mode(EncodeMode::Canonical);
    let mut out = Vec::new();
    normalized.encode(&mut out).unwrap(); // EncodeExt::encode -> canonical, per the mode
    println!("encode() under Canonical mode -> {out:02x?}");
    assert_eq!(out, [0x50, 0x12, 0x34]);

    // Or get a fresh, normalized value directly.
    let canon = received.to_canonical();
    assert!(canon.is_canonical());
    assert!(canon.canonical_diff().is_empty());

    // Built from scratch: the reserved field defaults to spec, so it's optional and canonical.
    let built = Frame::builder()
        .version(u4::new(3))
        .payload(0xBEEF)
        .build()
        .unwrap();
    assert!(built.is_canonical());
    println!("built: {built:#?}");

    println!("all checks passed");
}
