//! Property-based round-trip stress tests for `#[wire]`.
//!
//! Two invariants, over random inputs:
//! - **encode∘decode = identity**: any builder-constructed value survives
//!   `write` then `read` unchanged (catches bit-loss / mis-layout across the full
//!   value range, including catch-all enum values and derived counts).
//! - **decode∘encode = identity (bytes)**: for a fixed-size header, *any* byte
//!   string parses (the parser is total — dual-use) and re-encodes to the same
//!   bytes (the codec is a bijection on the wire word).
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite};
use bits::{BitEnum, Bits, bitflags, u4, wire};
use proptest::prelude::*;
use std::io::Cursor;

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum OpCode {
    Query,
    IQuery,
    Status,
    #[catch_all]
    Other(u4),
}

#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    qr: bool,
    aa: bool,
    tc: bool,
    rd: bool,
    ra: bool,
    z0: bool,
    z1: bool,
    z2: bool,
}

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    #[catch_all]
    Other(u4),
}

// opcode(4) + flags(8) + rcode(4) = 16 bits, fills the u16 group exactly.
#[wire(big, group(opcode, flags, rcode => u16))]
#[derive(Debug, Clone, PartialEq)]
struct Hdr {
    id: u16,
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,
    #[update(self.records.len() as u16)]
    count: u16,
    #[br(count = count)]
    #[builder(default)]
    records: Vec<u16>,
}

// A fixed-size (4-byte) header: id + the packed group word, no variable section.
#[wire(big, group(opcode, flags, rcode => u16))]
#[derive(Debug, Clone, PartialEq)]
struct Fixed {
    id: u16,
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,
}

proptest! {
    // encode∘decode = identity, across the full field-value range.
    #[test]
    fn header_roundtrip(
        id in any::<u16>(),
        op in 0u128..16,
        fl in any::<u8>(),
        rc in 0u128..16,
        records in prop::collection::vec(any::<u16>(), 0..12),
    ) {
        let h = Hdr::builder()
            .id(id)
            .opcode(OpCode::from_bits(op))
            .flags(Flags::from_bits(fl))
            .rcode(RCode::from_bits(rc))
            .records(records.clone())
            .build()
            .unwrap();

        let mut buf = Cursor::new(Vec::new());
        h.write(&mut buf).unwrap();
        let bytes = buf.into_inner();

        // derived count is exactly the section length on the wire
        // (layout: id[0..2], group word[2..4], count[4..6], records[6..])
        prop_assert_eq!(&bytes[4..6], &(records.len() as u16).to_be_bytes());

        let back = Hdr::read(&mut Cursor::new(&bytes)).unwrap();
        prop_assert_eq!(back, h);
    }

    // decode∘encode = identity (bytes): any 4 bytes parse and re-encode exactly.
    #[test]
    fn fixed_bytes_roundtrip(bytes in any::<[u8; 4]>()) {
        let parsed = Fixed::read(&mut Cursor::new(bytes.as_slice())).unwrap();
        let mut buf = Cursor::new(Vec::new());
        parsed.write(&mut buf).unwrap();
        let out = buf.into_inner();
        prop_assert_eq!(out.as_slice(), bytes.as_slice());
    }

    // The group word is a bijection: unpack∘pack over every field combination.
    #[test]
    fn group_word_is_bijection(word in any::<u16>()) {
        let bytes = [0u8, 0u8, (word >> 8) as u8, word as u8];
        let parsed = Fixed::read(&mut Cursor::new(bytes.as_slice())).unwrap();
        let mut buf = Cursor::new(Vec::new());
        parsed.write(&mut buf).unwrap();
        let out = buf.into_inner();
        prop_assert_eq!(&out[2..4], &word.to_be_bytes());
    }
}
