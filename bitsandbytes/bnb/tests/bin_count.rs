//! `count` (ROADMAP Phase 2, P2.2): a `#[br(count = <expr>)]` `Vec<T>` field reads
//! `expr` elements (the expr may name an earlier field); encode writes them all.
//! A count-bearing message is variable-length, so it implements `BitDecode`/
//! `BitEncode` but **not** `FixedBitLen`. Pairing count with `temp`/`calc` (so the
//! length field isn't stored) is a later chunk.

use bnb::{FixedBitLen, bin, u4, u12};

// Leaf-element Vec: a sub-byte tag (so the right-tool guard passes), a count
// field, then a byte payload that straddles byte boundaries (starts at bit 12).
#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Msg {
    tag: u4,
    n: u8,
    #[br(count = n)]
    data: Vec<u8>,
}

#[test]
fn count_drives_leaf_vec() {
    let m = Msg {
        tag: u4::new(0x5),
        n: 3,
        data: vec![0xAA, 0xBB, 0xCC],
    };
    let bytes = m.to_bytes().unwrap();
    let decoded = Msg::decode_exact(&bytes).unwrap();
    assert_eq!(decoded, m);
    assert_eq!(decoded.data.len(), 3);
}

#[test]
fn count_zero_reads_empty() {
    let m = Msg {
        tag: u4::new(0),
        n: 0,
        data: vec![],
    };
    let bytes = m.to_bytes().unwrap();
    assert_eq!(Msg::decode_exact(&bytes).unwrap(), m);
}

// Nested-element Vec: each element is itself a `#[bin]` message.
#[bin]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Record {
    a: u4,
    b: u12, // 16 bits, fixed
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Table {
    flags: u4, // sub-byte: guard passes, and records start at a non-byte offset
    count: u8,
    #[br(count = count)]
    #[nested]
    records: Vec<Record>,
}

#[test]
fn count_drives_nested_vec() {
    let t = Table {
        flags: u4::new(0xF),
        count: 2,
        records: vec![
            Record {
                a: u4::new(1),
                b: u12::new(2),
            },
            Record {
                a: u4::new(3),
                b: u12::new(4),
            },
        ],
    };
    let bytes = t.to_bytes().unwrap();
    assert_eq!(Table::decode_exact(&bytes).unwrap(), t);
}

#[test]
fn fixed_record_is_fixed_len_but_table_is_not() {
    // A fixed element type implements FixedBitLen...
    assert_eq!(<Record as FixedBitLen>::BIT_LEN, 16);
    // ...while the count-bearing Table does not (asserted at compile time by the
    // absence of a FixedBitLen impl; see trybuild ui/bin_count_not_fixed).
}
