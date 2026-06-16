//! Nested messages (ROADMAP Phase 1, chunk D): a `#[nested]` field is itself a
//! `BitDecode`/`BitEncode` message, recursed into (not a `Bits` leaf). The marker
//! is a Phase-1 mechanism; the end-state can auto-detect via universal impls.

use bnb::{BitDecode, BitEncode, BitReader, BitWriter, ErrorKind, FixedBitLen, u4, u12};

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct Inner {
    a: u4,
    b: u12, // 16 bits
}

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct Outer {
    tag: u4,
    #[nested]
    inner: Inner, // 16 bits, recursed into
    trailer: u12, // 4 + 16 + 12 = 32 bits
}

fn sample() -> Outer {
    Outer {
        tag: u4::new(0x5),
        inner: Inner {
            a: u4::new(0xA),
            b: u12::new(0xBCD),
        },
        trailer: u12::new(0xEEF),
    }
}

#[test]
fn nested_round_trips() {
    let o = sample();
    let mut w = BitWriter::new();
    o.bit_encode(&mut w).unwrap();
    let bytes = w.into_bytes();
    assert_eq!(bytes.len(), 4, "32 bits");

    let mut r = BitReader::new(&bytes);
    assert_eq!(Outer::bit_decode(&mut r).unwrap(), o);

    // The high-level entry points work through the nesting too.
    assert_eq!(Outer::decode_exact(&o.to_bytes().unwrap()).unwrap(), o);
}

#[test]
fn bit_len_sums_through_the_nesting() {
    assert_eq!(<Inner as FixedBitLen>::BIT_LEN, 16);
    assert_eq!(<Outer as FixedBitLen>::BIT_LEN, 4 + 16 + 12);
}

#[test]
fn nested_error_keeps_the_innermost_field_span() {
    // Truncate so Inner's `b` runs off the end: tag(4)=5, a(4)=A, then b needs 12.
    let short = [0x5A];
    let mut r = BitReader::new(&short);
    let err = Outer::bit_decode(&mut r).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    // Innermost (leaf) field wins the span — the failing field is Inner's `b`.
    // (Dotted paths like `inner.b` are a Phase-2 refinement.)
    assert_eq!(err.field, Some("b"));
}
