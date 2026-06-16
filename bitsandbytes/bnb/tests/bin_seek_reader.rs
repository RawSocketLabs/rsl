//! `SeekReader` (ROADMAP Phase 3b): a `SeekSource` over a `Read + Seek` (a file-like)
//! that seeks via `io::Seek` to the byte holding the bit cursor, with no buffering —
//! the large-file / container-format case. A seek-using message round-trips over it.

use bnb::{SeekReader, bin, u4};
use std::io::Cursor;

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    flags: u4,
    #[br(restore_position)]
    peek: u8,
    value: u16,
}

#[test]
fn seek_reader_over_a_file_like_source() {
    let wire = vec![0x5A, 0xBC, 0xD0]; // flags=5, value=0xABCD (restore_position layout)
    let mut src = SeekReader::new(Cursor::new(wire));
    let f = Frame::decode_from(&mut src).unwrap();
    assert_eq!(f.value, 0xABCD);
    assert_eq!(f.peek, 0xAB, "rewound and re-read via io::Seek");
}
