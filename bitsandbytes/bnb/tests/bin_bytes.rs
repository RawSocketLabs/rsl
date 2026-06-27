//! `bytes` integration (ROADMAP Phase 3, the `bytes` feature): zero-copy
//! `BytesReader`/`BytesWriter`. Decode from an owned `Bytes` frame; encode into a
//! writer you `freeze()` to a `Bytes` — the async/tokio framing path.
#![cfg(feature = "bytes")]

use bnb::{BitEncode, BytesReader, BytesWriter, bin, u4, u12};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    a: u4,
    b: u12,
}

#[test]
fn round_trip_through_bytes() {
    let f = Frame {
        a: u4::new(0xA),
        b: u12::new(0x123),
    };

    // Encode into a BytesWriter, then freeze to a zero-copy Bytes.
    let mut w = BytesWriter::new();
    f.bit_encode(&mut w).unwrap();
    let frozen = w.freeze();
    assert_eq!(&frozen[..], &[0xA1, 0x23]);

    // Decode from an owned Bytes via BytesReader.
    let mut r = BytesReader::new(frozen.clone());
    let decoded = Frame::decode(&mut r).unwrap();
    assert_eq!(decoded, f);
}
