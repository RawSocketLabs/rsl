//! `ctx` (ROADMAP Phase 2, P2.3, "Layer 1"): thread context from a parent into a
//! child's parse/serialize. A `#[bin(ctx(...))]` type gets inherent
//! `decode_with`/`encode_with` + a generated `…Ctx` struct, and does **not**
//! implement `BitDecode`/`BitEncode` (the core traits take no context). A
//! `#[br(ctx { … })]` field passes the context in.

use bits::{bin, u4};

// --- A child whose length depends on context passed by the parent (a TLV-ish
// shape: the parent reads a length/tag, the child is parsed accordingly). ---
#[bin(ctx(n: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Value {
    flag: u4,
    #[br(count = n)] // uses the ctx param
    data: Vec<u8>,
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Tlv {
    n: u8,
    #[br(ctx { n })] // `n` is a parent FIELD -> resolves to self.n on encode
    value: Value,
}

#[test]
fn ctx_threads_a_field_into_a_child() {
    let t = Tlv {
        n: 3,
        value: Value {
            flag: u4::new(0x5),
            data: vec![0x11, 0x22, 0x33],
        },
    };
    let bytes = t.to_bytes().unwrap();
    assert_eq!(Tlv::decode_exact(&bytes).unwrap(), t);
}

// --- A SEQUENCE-OF whose elements each need the parent's own context. ---
#[bin(ctx(version: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Item {
    kind: u4,
    #[br(count = version)]
    data: Vec<u8>,
}

#[bin(ctx(version: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Container {
    flags: u4,
    n: u16,
    #[br(count = n, ctx { version })] // `version` is a parent CTX PARAM -> stays a local
    items: Vec<Item>,
}

#[test]
fn ctx_threads_into_a_count_loop() {
    let c = Container {
        flags: u4::new(0xF),
        n: 2,
        items: vec![
            Item {
                kind: u4::new(1),
                data: vec![0xA, 0xB],
            },
            Item {
                kind: u4::new(2),
                data: vec![0xC, 0xD],
            },
        ],
    };
    let ctx = ContainerCtx { version: 2 };
    let bytes = c.to_bytes_with(ctx.clone()).unwrap();
    assert_eq!(Container::decode_with_exact(&bytes, ctx).unwrap(), c);
}
