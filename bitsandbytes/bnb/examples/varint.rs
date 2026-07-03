//! **varint** — the shipped **LEB128** codec, [`bnb::codecs::leb128`]: variable-length
//! integers (the encoding protobuf, DWARF, WASM, and git use), two ways. Per **field**:
//! one `parse_with`/`write_with` attribute pair, generic over the width (the field's
//! declared type — `u32` vs `u64` below — pins it). Per **type**: a `#[bin(codec = …)]`
//! newtype that carries the codec with it, so fields need no attributes at all (just
//! `#[brw(variable)]` in a fixed parent). Decoding is **bounded and overflow-checked**:
//! a hostile continuation run is a clean error, where a naive hand-rolled reader would
//! shift unbounded and panic in debug builds.
//!
//! (Rolling your own codec is still first-class — see the `parse_with` section of
//! [`bnb::guide::directives`], and `dns.rs` for a real one with DNS name compression.)
//!
//! Run with: `cargo run -p bitsandbytes --example varint`

use bnb::bin;
use bnb::bitstream::ErrorKind;

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Record {
    kind: u8,
    #[br(parse_with = bnb::codecs::leb128::parse)]
    #[bw(write_with = bnb::codecs::leb128::write)]
    length: u32, // one codec, two widths — the field type decides
    #[br(parse_with = bnb::codecs::leb128::parse)]
    #[bw(write_with = bnb::codecs::leb128::write)]
    timestamp: u64,
}

// --- the per-type form: annotate once, use as a plain field everywhere ----------

/// A LEB128-encoded u64 — the codec travels with the type.
#[bin(codec = bnb::codecs::leb128)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Varint(u64);

/// `kind` and `crc` are fixed-width; `#[brw(variable)]` tells the parent its width
/// isn't fixed (so it doesn't try to claim `FixedBitLen`).
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    kind: u8,
    #[brw(variable)]
    length: Varint, // no codec attributes — the type owns them
    crc: u16,
}

fn main() {
    // Small values pack into one byte; large ones grow only as needed — the point of LEB128.
    for &(length, timestamp) in &[(0u32, 0u64), (127, 128), (300, 1_000_000), (u32::MAX, 1)] {
        let r = Record {
            kind: 1,
            length,
            timestamp,
        };
        let bytes = r.to_bytes().unwrap();
        println!(
            "len={length} ts={timestamp} -> {} bytes  {bytes:02x?}",
            bytes.len()
        );
        assert_eq!(Record::decode_exact(&bytes).unwrap(), r);
    }

    // Bounded decode: a continuation run longer than the width allows (a u32 fits in
    // 5 LEB128 bytes) is a clean, position-aware error — never a panic, never a wrap.
    let hostile = [0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]; // kind, then 6×"more follows"
    let err = Record::decode_exact(&hostile).unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::Convert { message } if message.contains("unterminated"))
    );
    println!("hostile continuation run -> {err}");

    // Overflow is checked per width: 5 full bytes overflow a u32 field.
    let too_wide = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x00];
    let err = Record::decode_exact(&too_wide).unwrap_err();
    assert_eq!(err.field, Some("length"));
    println!("overflowing u32 field  -> {err}");

    // The per-type form: the newtype carries the codec, the field carries nothing.
    let f = Frame {
        kind: 1,
        length: Varint(300),
        crc: 0xBEEF,
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(bytes, [0x01, 0xAC, 0x02, 0xBE, 0xEF]); // kind | varint(300) | crc
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    assert_eq!(u64::from(Varint(300)), 300); // From both ways comes generated
    println!("codec-newtype frame    -> {bytes:02x?}");

    println!("all checks passed");
}
