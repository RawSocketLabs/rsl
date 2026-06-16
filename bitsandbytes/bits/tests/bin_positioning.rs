//! Positioning (ROADMAP Phase 2): forward `pad_*`/`align_*` directives skip bits or
//! align to a byte boundary around a field, with typed amounts from `bits::prelude`
//! (`4.bits()`, `1.bytes()`). Backward `seek`/`restore_position` need `SeekSource`
//! (Phase 3).

use bits::prelude::*;
use bits::{bin, u4};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    tag: u4,
    #[br(pad_before = 4.bits())] // 4 reserved bits between tag and value
    value: u8,
    #[br(align_after)] // pad to a byte boundary after the (here already-aligned) field
    trailer: u4,
}

#[test]
fn pad_and_align_round_trip() {
    let f = Frame {
        tag: u4::new(0x5),
        value: 0xAB,
        trailer: u4::new(0x3),
    };
    let bytes = f.to_bytes().unwrap();
    // tag(0101) pad(0000) | value(0xAB) | trailer(0011) pad-to-byte(0000)
    assert_eq!(bytes, [0x50, 0xAB, 0x30]);
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
}

// A whole-byte pad expressed in bytes.
#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Spaced {
    a: u4,
    #[br(pad_before = 1.bytes())] // skip a full byte
    b: u4,
}

#[test]
fn byte_sized_pad() {
    let f = Spaced {
        a: u4::new(0xA),
        b: u4::new(0xB),
    };
    let bytes = f.to_bytes().unwrap();
    // a(1010) pad-hi(0000) | pad-lo(0000 0000)? -> a at 0..4, pad 4..12, b 12..16
    // byte0 = 1010_0000, byte1 = 0000_1011
    assert_eq!(bytes, [0xA0, 0x0B]);
    assert_eq!(Spaced::decode_exact(&bytes).unwrap(), f);
}
