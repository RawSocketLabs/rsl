use bnb::{BitBuf, BitDecode, BitEncode, BitWriter, bin, u4};

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

/// `count` six-byte messages numbered from `first`, as one byte string.
fn numbered(first: u32, count: u32) -> Vec<u8> {
    (first..first + count)
        .flat_map(|b| LeMsg { a: 7, b }.to_bytes().unwrap())
        .collect()
}

#[test]
fn an_unbounded_buffer_grows_rather_than_move_a_backlog() {
    // A hundred unread messages in an allocation that fits them exactly.
    let mut buffer = BitBuf::new();
    buffer.push(&numbered(0, 100)).unwrap();
    let mut most = 0;
    for step in 0..1_000_u32 {
        let message = buffer.pull::<LeMsg>().unwrap().unwrap();
        assert_eq!(message.b, step, "in order, none lost");
        buffer.push(&numbered(100 + step, 1)).unwrap();
        most = most.max(buffer.buf.len());
        // What is consumed is reclaimed by the time it matches what is unread, so
        // the buffer holds under twice its backlog and one push.
        let live = buffer.buf.len() - buffer.cursor / 8;
        assert_eq!(live, 600);
        assert!(buffer.buf.len() < 2 * live + 6, "step {step}");
    }
    // And the backlog was not slid down on every push: the consumed prefix was
    // left in place until, with 594 bytes unread before a push, it reached 594.
    assert_eq!(most, 588 + 600);
}

#[test]
fn an_unbounded_buffer_reclaims_once_the_consumed_bytes_match_the_unread() {
    let mut buffer = BitBuf::new();
    buffer.push(&numbered(0, 4)).unwrap();
    buffer.pull::<LeMsg>().unwrap().unwrap();
    // One consumed, three unread: the next push leaves the consumed bytes be.
    buffer.push(&[]).unwrap();
    assert_eq!(buffer.cursor, 48);
    buffer.pull::<LeMsg>().unwrap().unwrap();
    // Two and two: reclaimed, and the unread messages are still there in order.
    buffer.push(&[]).unwrap();
    assert_eq!((buffer.cursor, buffer.buf.len()), (0, 12));
    assert_eq!(buffer.pull::<LeMsg>().unwrap().unwrap().b, 2);
    assert_eq!(buffer.pull::<LeMsg>().unwrap().unwrap().b, 3);
}

#[test]
fn the_reader_behind_an_attempt_lends_as_the_slice_reader_does() {
    use crate::Source as _;
    use crate::bitstream::IncrementalReader;

    let mut reader = IncrementalReader(crate::BitReader::new(&[1, 2, 3]));
    assert_eq!(reader.aligned_run(2), Some(&[1, 2][..]));
    assert_eq!(reader.bit_pos(), 16);
    assert_eq!(reader.aligned_run(2), None);
}

#[test]
fn a_buffer_lends_the_whole_bytes_it_holds_and_moves_past_them() {
    use crate::Source as _;

    // The hook behind magic dispatch, integer runs and the window a fixed-length
    // message decodes from: without it each falls back to a slower path that
    // gives the same answer, so only this says the buffer lends at all.
    let mut buffer = BitBuf::new();
    buffer.push(&[1, 2, 3, 4, 5]).unwrap();
    assert_eq!(buffer.aligned_run(2), Some(&[1, 2][..]));
    assert_eq!(buffer.bit_pos(), 16);
    // Not more than it holds, and not from inside a byte.
    assert_eq!(buffer.aligned_run(4), None);
    buffer.read_bits(4).unwrap();
    assert_eq!(buffer.aligned_run(1), None);
    assert_eq!(buffer.bit_pos(), 20);
}

#[test]
fn a_bounded_buffer_reclaims_whenever_new_bytes_would_not_fit() {
    // Room for four messages, full; one is consumed, three are unread.
    let mut buffer = BitBuf::bounded(24);
    buffer.push(&numbered(0, 4)).unwrap();
    buffer.pull::<LeMsg>().unwrap().unwrap();
    // The fifth fits only where the first was.
    buffer.push(&numbered(4, 1)).unwrap();
    assert_eq!((buffer.cursor, buffer.buf.len()), (0, 24));
    for b in 1..5 {
        assert_eq!(buffer.pull::<LeMsg>().unwrap().unwrap().b, b);
    }
    // With room to spare, and fewer bytes consumed than unread, nothing moves.
    let mut buffer = BitBuf::bounded(48);
    buffer.push(&numbered(0, 4)).unwrap();
    buffer.pull::<LeMsg>().unwrap().unwrap();
    buffer.push(&numbered(4, 1)).unwrap();
    assert_eq!((buffer.cursor, buffer.buf.len()), (48, 30));
    // Nor when the new bytes fill exactly what is left.
    let mut buffer = BitBuf::bounded(30);
    buffer.push(&numbered(0, 4)).unwrap();
    buffer.pull::<LeMsg>().unwrap().unwrap();
    buffer.push(&numbered(4, 1)).unwrap();
    assert_eq!((buffer.cursor, buffer.buf.len()), (48, 30));
}

#[test]
fn pull_is_none_until_a_whole_message_arrives_then_reclaims() {
    let m = LeMsg {
        a: 0x1234,
        b: 0xDEAD_BEEF,
    };
    let bytes = m.to_bytes().unwrap();

    let mut bb = BitBuf::new();
    bb.push(&bytes[..3]).unwrap(); // only part of the message
    assert_eq!(bb.pull::<LeMsg>().unwrap(), None); // wait for more — buffer untouched
    assert_eq!(bb.bit_len(), 24);

    bb.push(&bytes[3..]).unwrap(); // the rest
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
        bb.push(chunk).unwrap();
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
    bb.push(&[1, 2, 3]).unwrap();
    assert_eq!(bb.bit_len(), 24);
    bb.clear();
    assert!(bb.is_empty());
}

// BitBuf is a Source: it reads through the same `bit_decode` entry the renamed `decode` uses.
// The default-order buffer reads a big message; `with_layout` reads a little one (this also
// proves byte order is applied exactly once — no double-ordering in the Source delegation).
#[test]
fn reads_as_a_source_respecting_layout() {
    // big message via a default (msb/big) BitBuf
    let f = Frame {
        tag: u4::new(0xC),
        val: 0x9A,
    };
    let mut bb = BitBuf::new();
    bb.push(&f.to_bytes().unwrap()).unwrap();
    assert_eq!(<Frame as BitDecode>::bit_decode(&mut bb).unwrap(), f);

    // little message via a layout-configured BitBuf (byte-aligned, so compact fully drains)
    let m = LeMsg {
        a: 0x1234,
        b: 0xDEAD_BEEF,
    };
    let mut bb = BitBuf::new().with_layout(<LeMsg as BitEncode>::LAYOUT);
    bb.push(&m.to_bytes().unwrap()).unwrap();
    let got = <LeMsg as BitDecode>::bit_decode(&mut bb).unwrap();
    assert_eq!(got, m); // would be byte-swapped if ordering double-applied
    bb.compact(); // Source path doesn't auto-reclaim
    assert!(bb.is_empty());
}

#[test]
fn a_refused_bit_read_leaves_the_cursor_where_it_was() {
    use crate::{ErrorKind, Source as _};

    let mut buffer = BitBuf::new();
    buffer.push(&[0xAB, 0xCD]).unwrap();
    assert_eq!(buffer.read_bits(4).unwrap(), 0xA);
    // Past the end, as a slice reads it: what was asked, what was left, and where.
    let error = buffer.read_bits(13).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::UnexpectedEof {
            needed: 13,
            remaining: 12
        }
    );
    assert_eq!(error.at, 4);
    let error = buffer.read_bits(129).unwrap_err();
    assert_eq!(error.kind, ErrorKind::TooWide { width: 129 });
    assert_eq!((error.at, error.field), (4, None));
    assert_eq!(buffer.bit_pos(), 4, "a refused read consumes nothing");
    assert_eq!(buffer.read_bits(12).unwrap(), 0xBCD);
}

// BitBuf is a `SeekSource`, so a `restore_position` message decodes over it through the
// `decode` cursor path — exercising BitBuf's `seek_to_bit` (the rewind).
#[test]
fn as_a_seek_source_a_restore_position_message_decodes() {
    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct Peeked {
        #[br(restore_position)]
        tag: u8,
        full: u16,
    }
    let mut bb = BitBuf::new();
    bb.push(&[0xAB, 0xCD]).unwrap();
    let p = Peeked::decode(&mut bb).unwrap();
    assert_eq!((p.tag, p.full), (0xAB, 0xABCD));
}

// --- bounded (alloc-once) mode -----------------------------------------------------

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Two {
    v: u16,
}

#[test]
fn bounded_push_respects_capacity_then_reclaims_in_place() {
    use bnb::CapacityError;
    let mut bb = BitBuf::bounded(4);
    assert_eq!(bb.capacity(), Some(4));
    bb.push(&[0x00, 0x01]).unwrap(); // 2 bytes
    bb.push(&[0x00, 0x02]).unwrap(); // 4 bytes — full
    // a 5th byte can't fit until something is drained
    assert!(matches!(
        bb.push(&[0xFF]),
        Err(CapacityError { cap: 4, .. })
    ));
    // drain one message → 2 live bytes; the dead prefix is reclaimed in place to fit more
    assert_eq!(bb.pull::<Two>().unwrap(), Some(Two { v: 1 }));
    bb.push(&[0x00, 0x03]).unwrap();
    assert_eq!(bb.pull::<Two>().unwrap(), Some(Two { v: 2 }));
    assert_eq!(bb.pull::<Two>().unwrap(), Some(Two { v: 3 }));
    assert!(bb.is_empty());
}

#[test]
fn grow_raises_a_bounded_capacity() {
    let mut bb = BitBuf::bounded(2);
    bb.push(&[0x00, 0x01]).unwrap();
    assert!(bb.push(&[0x02]).is_err()); // full at 2
    bb.grow(2); // the one explicit allocation
    assert_eq!(bb.capacity(), Some(4));
    bb.push(&[0x02, 0x03]).unwrap();
    assert_eq!(bb.bit_len(), 32);
}

#[test]
fn unbounded_push_never_fails() {
    let mut bb = BitBuf::new();
    assert_eq!(bb.capacity(), None);
    bb.push(&[1, 2, 3]).unwrap(); // no cap → grows, never errors
    assert_eq!(bb.bit_len(), 24);
}

#[test]
fn a_bounded_clone_allocates_its_whole_cap_and_reads_on_where_the_original_was() {
    // Unread bytes short of the cap: a plain `Vec` clone would reserve only those.
    let mut original = BitBuf::bounded(24);
    original.push(&numbered(0, 3)).unwrap();
    assert_eq!(original.pull::<LeMsg>().unwrap().unwrap().b, 0);
    let mut copy = original.clone();
    assert_eq!(copy.capacity(), Some(24));
    assert!(
        copy.buf.capacity() >= 24,
        "a bounded clone keeps the alloc-once promise"
    );
    copy.push(&numbered(3, 1)).unwrap();
    for b in 1..4 {
        assert_eq!(copy.pull::<LeMsg>().unwrap().unwrap().b, b);
    }
    assert_eq!(
        original.pull::<LeMsg>().unwrap().unwrap().b,
        1,
        "the original is its own"
    );
}

#[test]
fn a_streaming_push_pull_loop_stays_within_a_tiny_cap() {
    // Pushed one message at a time and drained immediately, a bounded buffer reuses the same
    // allocation forever: each push fits because the prior message was reclaimed in place.
    let mut bb = BitBuf::bounded(2);
    for i in 0..100u16 {
        bb.push(&i.to_be_bytes()).unwrap();
        assert_eq!(bb.pull::<Two>().unwrap(), Some(Two { v: i }));
    }
    assert!(bb.is_empty());
}
