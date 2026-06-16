//! `if` (ROADMAP Phase 2, P2.5): a conditional `Option<T>` field. On decode it is
//! `Some(read)` when the condition (over earlier fields, as locals) holds, else
//! `None` (consuming nothing). On encode the `Option`'s presence drives the write.

use bnb::{bin, u4, u12};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    flags: u4,
    has_extra: u4,
    #[br(if(has_extra != u4::new(0)))]
    extra: Option<u16>,
}

#[test]
fn present_when_condition_holds() {
    let f = Frame {
        flags: u4::new(1),
        has_extra: u4::new(1),
        extra: Some(0xBEEF),
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(bytes.len(), 3, "4 + 4 + 16 = 24 bits");
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
}

#[test]
fn absent_consumes_nothing() {
    let f = Frame {
        flags: u4::new(2),
        has_extra: u4::new(0),
        extra: None,
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(bytes.len(), 1, "4 + 4 = 8 bits, no extra on the wire");
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
}

// A conditional **nested message** field.
#[bin]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Sub {
    a: u4,
    b: u12,
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame2 {
    tag: u4,
    present: u4,
    #[br(if(present != u4::new(0)))]
    #[nested]
    sub: Option<Sub>,
}

#[test]
fn conditional_nested_message() {
    let some = Frame2 {
        tag: u4::new(1),
        present: u4::new(1),
        sub: Some(Sub {
            a: u4::new(3),
            b: u12::new(0x123),
        }),
    };
    assert_eq!(
        Frame2::decode_exact(&some.to_bytes().unwrap()).unwrap(),
        some
    );

    let none = Frame2 {
        tag: u4::new(1),
        present: u4::new(0),
        sub: None,
    };
    assert_eq!(
        Frame2::decode_exact(&none.to_bytes().unwrap()).unwrap(),
        none
    );
}
