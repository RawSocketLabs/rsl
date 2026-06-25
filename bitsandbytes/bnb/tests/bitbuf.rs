//! `BitBuf` — a push/pull, bit-aware incremental decode buffer.

use bnb::{BitBuf, BitEncode, BitWriter, bin, u4};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Frame {
    tag: u4,
    val: u8,
} // 12 bits — a non-byte-aligned boundary

#[bin(little)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct LeMsg {
    a: u16,
    b: u32,
} // little-endian, byte-aligned (6 bytes)

#[test]
fn pull_is_none_until_a_whole_message_arrives_then_reclaims() {
    let m = LeMsg {
        a: 0x1234,
        b: 0xDEAD_BEEF,
    };
    let bytes = m.to_bytes().unwrap();

    let mut bb = BitBuf::new();
    bb.push(&bytes[..3]); // only part of the message
    assert_eq!(bb.pull::<LeMsg>().unwrap(), None); // wait for more — buffer untouched
    assert_eq!(bb.bit_len(), 24);

    bb.push(&bytes[3..]); // the rest
    assert_eq!(bb.pull::<LeMsg>().unwrap(), Some(m)); // decodes (little-endian honored via LAYOUT)
    assert!(bb.is_empty()); // consumed bytes reclaimed
    assert_eq!(bb.pull::<LeMsg>().unwrap(), None);
}

#[test]
fn reassembles_sub_byte_boundary_messages_across_pushes() {
    let f1 = Frame {
        tag: u4::new(0xA),
        val: 0x12,
    };
    let f2 = Frame {
        tag: u4::new(0xB),
        val: 0x34,
    };
    // Pack contiguously: 24 bits / 3 bytes, with f2 starting at bit 12 (mid-byte).
    let mut w = BitWriter::new();
    f1.bit_encode(&mut w).unwrap();
    f2.bit_encode(&mut w).unwrap();
    let wire = w.into_bytes();

    let mut bb = BitBuf::new();
    let mut out = Vec::new();
    // f1 spans the chunk boundary; the bit cursor keeps f2's sub-byte alignment.
    for chunk in [&wire[0..1], &wire[1..3]] {
        bb.push(chunk);
        while let Some(f) = bb.pull::<Frame>().unwrap() {
            out.push(f);
        }
    }
    assert_eq!(out, vec![f1, f2]);
    assert!(bb.is_empty());
}

#[test]
fn clear_and_capacity() {
    let mut bb = BitBuf::with_capacity(64);
    bb.push(&[1, 2, 3]);
    assert_eq!(bb.bit_len(), 24);
    bb.clear();
    assert!(bb.is_empty());
}
