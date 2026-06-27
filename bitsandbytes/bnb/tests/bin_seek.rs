//! `#[br(seek = <bits>)]` — read a field at an absolute bit offset (following a
//! pointer), and `#[br(dbg)]` — trace a field as it decodes.
//!
//! `seek` is a read-side primitive (the writer is append-only). Paired with
//! `restore_position` it reads at an offset and returns, so later fields continue from
//! where they were — the classic offset-table / pointer-follow shape. Because it seeks,
//! `decode` is bound on `SeekSource`; the slice entry points always qualify.

use bnb::{bin, prelude::*};

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Record {
    /// Byte offset of `target` within the buffer.
    ptr: u8,
    /// Read the byte at `ptr`, then rewind so `next` reads right after `ptr`.
    #[br(seek = ptr.bytes(), restore_position)]
    target: u8,
    next: u8,
}

#[test]
fn seek_follows_a_pointer_then_restore_position_returns() {
    // [ptr=3][next=0x11][filler=0x22][target=0xAB]
    let buf = [0x03, 0x11, 0x22, 0xAB];
    // `peek` doesn't require full consumption — seek/restore leave bytes 2.. untouched.
    let p = Record::peek(&buf).unwrap();
    assert_eq!(p.ptr, 3);
    assert_eq!(p.target, 0xAB); // read from byte offset 3 via seek
    assert_eq!(p.next, 0x11); // restore_position rewound, so `next` is byte 1
}

#[test]
fn seek_reads_at_an_offset_without_restore() {
    // Without restore_position the cursor stays past the seeked read; useful when the
    // pointee is the tail of the message. Here `target` lands exactly at the end.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Tail {
        off: u8,
        #[br(seek = off.bytes())]
        target: u8,
    }
    let t = Tail::peek(&[0x02, 0x00, 0x7E]).unwrap();
    assert_eq!(t.off, 2);
    assert_eq!(t.target, 0x7E); // byte offset 2
}

// `dbg` is a read-side diagnostic: it emits a `tracing` event but is otherwise inert —
// decode and encode are unaffected (enable with `RUST_LOG=bnb::dbg=trace`).
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Dbg {
    a: u8,
    #[br(dbg)]
    b: u16,
}

#[test]
fn dbg_is_inert_for_the_codec() {
    let m = Dbg::decode_exact(&[0x01, 0xCA, 0xFE]).unwrap();
    assert_eq!(m, Dbg { a: 1, b: 0xCAFE });
    assert_eq!(m.to_bytes().unwrap(), [0x01, 0xCA, 0xFE]); // write side untouched
}
