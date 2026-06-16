//! `restore_position` + `SeekSource` (ROADMAP Phase 3): `#[br(restore_position)]`
//! reads a field (a peek), then rewinds so a later field re-reads the same bytes.
//! It needs a seekable source (the slice `BitReader`); on a forward stream it errors
//! (`ErrorKind::NotSeekable`), and `#[bin(forward_only)]` makes it a compile error.

use bnb::{bin, u4};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    flags: u4,
    #[br(restore_position)]
    peek: u8, // peek the byte at the value's offset...
    value: u16, // ...then read the full value from the same offset
}

#[test]
fn restore_position_peeks_then_rereads() {
    let f = Frame {
        flags: u4::new(0x5),
        peek: 0xAB, // the high byte of `value`
        value: 0xABCD,
    };
    let bytes = f.to_bytes().unwrap();
    // flags(0101) | value(0xABCD, 16 bits) — `peek` overlaps `value`, not written.
    assert_eq!(bytes, [0x5A, 0xBC, 0xD0]);

    let decoded = Frame::decode_exact(&bytes).unwrap();
    assert_eq!(decoded.value, 0xABCD);
    assert_eq!(decoded.peek, 0xAB, "peeked the high byte at value's offset");
    assert_eq!(decoded, f);
}
