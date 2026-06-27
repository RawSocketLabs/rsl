//! `BufSource` (ROADMAP Phase 3): a seekable `Source` over a forward `Read`. It
//! retains read bytes, so a seek-using message (`restore_position`) works over a
//! non-seekable stream by seeking within the buffer, reading more on demand — and
//! is bounded by a retention cap (`ErrorKind::BufferFull`).

use bnb::{BufSource, ErrorKind, Source, bin, u4};

// A forward-only reader (a socket-like stream) yielding one byte per `read`.
struct Chunked {
    data: Vec<u8>,
    pos: usize,
}
impl std::io::Read for Chunked {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.data.len() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.data[self.pos];
        self.pos += 1;
        Ok(1)
    }
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    flags: u4,
    #[br(restore_position)]
    peek: u8,
    value: u16,
}

#[test]
fn seek_using_message_over_a_nonseekable_stream() {
    // Wire bytes from the restore_position round-trip: flags=5, value=0xABCD.
    let wire = vec![0x5A, 0xBC, 0xD0];
    let mut src = BufSource::new(Chunked { data: wire, pos: 0 });
    let f = Frame::decode(&mut src).unwrap();
    assert_eq!(f.value, 0xABCD);
    assert_eq!(f.peek, 0xAB, "the rewind re-read retained bytes");
}

#[test]
fn retention_cap_bounds_the_buffer() {
    // A 1-byte cap; reading a 16-bit value needs 2 bytes -> BufferFull.
    let mut src = BufSource::with_cap(
        Chunked {
            data: vec![0xFF; 8],
            pos: 0,
        },
        1,
    );
    let err = src.read_bits(16).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::BufferFull { cap: 1 }));
}
