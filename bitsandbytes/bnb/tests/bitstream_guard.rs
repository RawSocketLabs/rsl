//! The right-tool guard on `#[derive(BitDecode/BitEncode)]`.
//!
//! An all-byte-aligned struct is **rejected** at compile time (proof:
//! `tests/ui/bitstream_byte_aligned.rs`), steering the author to `#[binrw]`/
//! `#[wire]`. The `#[bit_stream(allow_byte_aligned)]` escape hatch re-enables it
//! for the caller who really means it — verified here.

use bnb::{BitDecode, BitEncode, BitReader, BitWriter};

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq)]
#[bit_stream(allow_byte_aligned)]
struct ForcedBytes {
    a: u8,
    b: u16,
}

#[test]
fn override_reenables_byte_aligned() {
    let v = ForcedBytes { a: 0xAB, b: 0xCDEF };

    let mut w = BitWriter::new();
    v.bit_encode(&mut w).unwrap();
    let bytes = w.into_bytes();
    assert_eq!(bytes, [0xAB, 0xCD, 0xEF], "big-endian, byte-for-byte");

    let mut r = BitReader::new(&bytes);
    assert_eq!(ForcedBytes::bit_decode(&mut r).unwrap(), v);
}
