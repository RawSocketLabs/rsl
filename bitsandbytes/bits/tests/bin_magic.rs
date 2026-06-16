//! `magic` (ROADMAP Phase 2, P2.1): a leading constant verified on read and
//! emitted on write. Bit-aware — the magic may be sub-byte (`u3`), which binrw
//! cannot express, and which legitimately keeps the struct on the bit-stream codec.

use bits::{ErrorKind, bin, u3, u4, u12};

// A byte-aligned magic in front of a sub-byte message.
#[bin(magic = 0x7Eu8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Framed {
    version: u4,
    payload_len: u12,
}

#[test]
fn magic_round_trips() {
    let f = Framed {
        version: u4::new(4),
        payload_len: u12::new(100),
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(bytes[0], 0x7E, "magic emitted first");
    assert_eq!(Framed::decode_exact(&bytes).unwrap(), f);
    // The magic counts toward the const length: 8 + 4 + 12 = 24 bits = 3 bytes.
    assert_eq!(<Framed as bits::FixedBitLen>::BIT_LEN, 24);
    assert_eq!(bytes.len(), 3);
}

#[test]
fn magic_mismatch_errors() {
    let mut bytes = Framed {
        version: u4::new(4),
        payload_len: u12::new(0),
    }
    .to_bytes()
    .unwrap();
    bytes[0] = 0x5A; // corrupt the magic
    let err = Framed::decode_exact(&bytes).unwrap_err();
    assert!(matches!(
        err.kind,
        ErrorKind::BadMagic {
            expected: 0x7E,
            found: 0x5A
        }
    ));
    assert_eq!(err.field, Some("magic"));
    assert_eq!(err.at, 0);
}

// A sub-byte (3-bit) magic. The data field is byte-aligned, so without the magic
// the right-tool guard would fire — the sub-byte magic must suppress it.
#[bin(magic = u3::new(0b110))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct SubByteMagic {
    body: u8,
}

#[test]
fn sub_byte_magic_round_trips() {
    let s = SubByteMagic { body: 0xAB };
    let bytes = s.to_bytes().unwrap();
    assert_eq!(bytes.len(), 2, "3 + 8 = 11 bits -> 2 bytes");
    assert_eq!(bytes[0] >> 5, 0b110, "the 3-bit magic leads");
    assert_eq!(SubByteMagic::decode_exact(&bytes).unwrap(), s);
}
