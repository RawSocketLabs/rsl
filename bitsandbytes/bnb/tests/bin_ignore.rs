//! `#[brw(ignore)]`: a field that is neither read nor written — `Default::default()`
//! on read (no input consumed), skipped on write. Zero wire bits, but still a stored
//! (and builder) field. Spelled with `brw` because it applies to both directions.

use bnb::{bin, u4, u12};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    tag: u4,
    payload: u12,
    #[brw(ignore)]
    note: u32, // in-memory metadata, never serialized
}

#[test]
fn ignored_field_is_not_on_the_wire() {
    let f = Frame {
        tag: u4::new(0x5),
        payload: u12::new(0x123),
        note: 0xDEAD_BEEF,
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(
        bytes.len(),
        2,
        "tag(4) + payload(12) = 16 bits; note skipped"
    );

    let decoded = Frame::decode_exact(&bytes).unwrap();
    assert_eq!(decoded.tag, f.tag);
    assert_eq!(decoded.payload, f.payload);
    assert_eq!(decoded.note, 0, "defaulted on read, not the original value");
}

#[test]
fn ignored_field_is_still_a_builder_field() {
    let f = Frame::builder()
        .tag(u4::new(1))
        .payload(u12::new(2))
        .note(7)
        .build()
        .unwrap();
    assert_eq!(f.note, 7);
}
