//! `#[reserved]` / `#[reserved_with]` (ROADMAP Phase 2, P2.7): reserved bits — on
//! the wire (the type gives the width) but not stored. Read and discarded
//! (lenient — a non-zero value isn't rejected), written as 0 (or a given pattern).
//! Dropped from the struct and the builder.

use bnb::{bin, u3, u4};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    version: u4,
    #[reserved]
    _rsv: u4,
    payload: u8,
}

#[test]
fn reserved_is_zero_lenient_and_not_stored() {
    // No `_rsv` field on the struct (compile-time proof).
    let f = Frame {
        version: u4::new(5),
        payload: 0xAB,
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(bytes.len(), 2, "4 + 4 + 8 = 16 bits");
    assert_eq!(bytes[0], 0x50, "version(0101) then reserved(0000)");
    assert_eq!(bytes[1], 0xAB);
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    // Lenient: non-zero reserved bits are tolerated and discarded.
    assert_eq!(Frame::decode_exact(&[0x5F, 0xAB]).unwrap(), f);
}

#[test]
fn reserved_not_in_builder() {
    let f = Frame::builder()
        .version(u4::new(1))
        .payload(0x22)
        .build()
        .unwrap();
    assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame2 {
    tag: u4,
    #[reserved_with(u3::new(0b111))]
    _must_be_one: u3,
    rest: u4,
}

#[test]
fn reserved_with_writes_the_pattern() {
    let f = Frame2 {
        tag: u4::new(0xA),
        rest: u4::new(0x5),
    };
    let bytes = f.to_bytes().unwrap();
    // tag(1010) + reserved(111) + rest MSB(0) = 1010_1110 = 0xAE
    assert_eq!(bytes[0], 0xAE);
    assert_eq!(Frame2::decode_exact(&bytes).unwrap(), f);
}
