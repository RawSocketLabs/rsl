//! **varint** — the shipped **LEB128** codec, [`bnb::codecs::leb128`]: variable-length
//! integers (the encoding protobuf, DWARF, WASM, and git use) as one attribute pair per
//! field, no hand-rolled loop. The codec is generic over the width — the field's declared
//! type (`u32` vs `u64` below) pins it — and decoding is **bounded and overflow-checked**:
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

    println!("all checks passed");
}
