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
    let mut src = BufSource::with_capacity(
        Chunked {
            data: vec![0xFF; 8],
            pos: 0,
        },
        1,
    );
    let err = src.read_bits(16).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::BufferFull { cap: 1 }));
}

/// A reader of `count` 4-byte big-endian counters, 0 upward, behind a 16-byte cap.
fn counters(count: u32) -> BufSource<Chunked> {
    let data = (0..count).flat_map(u32::to_be_bytes).collect();
    BufSource::with_capacity(Chunked { data, pos: 0 }, 16)
}

#[test]
fn compacting_between_messages_decodes_a_stream_longer_than_the_cap() {
    // 40 messages are 160 bytes, ten times the cap: the cap bounds what is retained
    // at once, not how much may ever go by.
    let mut src = counters(40);
    for seq in 0..40_u32 {
        assert_eq!(src.read_bits(32).unwrap(), u128::from(seq));
        src.compact();
    }
    assert_eq!(src.bit_pos(), 40 * 32, "positions stay stream-absolute");
    assert!(matches!(
        src.read_bits(8).unwrap_err().kind,
        ErrorKind::UnexpectedEof { .. }
    ));
}

#[test]
fn without_compaction_the_cap_counts_every_byte_read() {
    let mut src = counters(40);
    for seq in 0..4_u32 {
        assert_eq!(src.read_bits(32).unwrap(), u128::from(seq));
    }
    let err = src.read_bits(32).unwrap_err();
    assert_eq!((err.kind, err.at), (ErrorKind::BufferFull { cap: 16 }, 128));
    // Nothing was lost: releasing the consumed bytes lets the same read succeed.
    src.compact();
    assert_eq!(src.read_bits(32).unwrap(), 4);
}

#[test]
fn seeking_before_the_compacted_bytes_is_not_seekable() {
    let mut src = counters(4);
    src.read_bits(32).unwrap();
    src.read_bits(32).unwrap();
    src.compact(); // bytes 0..8 are gone
    for gone in [0, 32, 63] {
        let err = src.seek_to_bit(gone).unwrap_err();
        assert_eq!((err.kind, err.at), (ErrorKind::NotSeekable, 64));
    }
    assert_eq!(src.bit_pos(), 64, "a refused seek leaves the cursor alone");
    assert_eq!(src.read_bits(32).unwrap(), 2);
}

/// A reader that hands out as many bytes as the caller's buffer holds and counts them.
struct Greedy {
    data: Vec<u8>,
    pos: usize,
}
impl std::io::Read for Greedy {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[test]
fn a_compacted_source_still_reads_no_further_than_the_field_needs() {
    // A caller that resumes the raw reader after decoding relies on this.
    let reader = Greedy {
        data: (0..64).collect(),
        pos: 0,
    };
    let mut src = BufSource::with_capacity(reader, 16);
    for byte in 0..8_u8 {
        assert_eq!(src.read_bits(8).unwrap(), u128::from(byte));
        src.compact();
    }
    assert_eq!(src.inner.pos, 8, "bytes taken from the reader");
}

#[test]
fn eof_after_compaction_reports_the_bits_actually_left() {
    let mut src = BufSource::new(&[0, 1, 2, 3, 4, 5][..]);
    src.read_bits(32).unwrap();
    src.compact();
    let err = src.read_bits(32).unwrap_err();
    assert_eq!(
        (err.kind, err.at),
        (
            ErrorKind::UnexpectedEof {
                needed: 32,
                remaining: 16
            },
            32
        )
    );
}

#[test]
fn seeking_within_the_retained_bytes_still_works_after_compaction() {
    let mut src = counters(4);
    src.read_bits(32).unwrap();
    src.compact(); // bytes 0..4 are gone; the buffer now starts at byte 4
    assert_eq!(src.read_bits(32).unwrap(), 1);
    assert_eq!(src.read_bits(32).unwrap(), 2);
    src.seek_to_bit(32).unwrap(); // absolute bit 32 is the oldest retained byte
    assert_eq!(src.read_bits(32).unwrap(), 1);
    src.seek_to_bit(64).unwrap();
    assert_eq!(src.read_bits(32).unwrap(), 2);
    assert_eq!(src.bit_pos(), 96);
}

#[test]
fn compacting_mid_byte_keeps_the_partially_consumed_byte() {
    let data = vec![0x12, 0x34, 0x56, 0x78];
    let mut src = BufSource::with_capacity(Chunked { data, pos: 0 }, 2);
    assert_eq!(src.read_bits(12).unwrap(), 0x123);
    src.compact(); // drops byte 0, keeps byte 1: its low nibble is unread
    assert_eq!(src.bit_pos(), 12);
    assert_eq!(src.read_bits(12).unwrap(), 0x456);
    src.seek_to_bit(8).unwrap(); // the start of the kept byte is still there
    assert_eq!(src.read_bits(8).unwrap(), 0x34);
    assert_eq!(src.seek_to_bit(7).unwrap_err().kind, ErrorKind::NotSeekable);
}

#[test]
fn compacting_after_a_seek_past_the_retained_bytes_loses_nothing() {
    let data = (0..8).collect();
    let mut src = BufSource::with_capacity(Chunked { data, pos: 0 }, 8);
    assert_eq!(src.read_bits(8).unwrap(), 0);
    src.seek_to_bit(40).unwrap(); // bytes 1..5 have not been read from the reader yet
    src.compact();
    assert_eq!(src.read_bits(8).unwrap(), 5);
    assert_eq!(src.bit_pos(), 48);
}

#[test]
fn over_wide_read_is_rejected() {
    let mut src = BufSource::new(Chunked {
        data: vec![0u8; 32],
        pos: 0,
    });
    assert!(matches!(
        src.read_bits(129).unwrap_err().kind,
        ErrorKind::TooWide { width: 129 }
    ));
}

#[test]
fn running_out_mid_field_at_eof_is_unexpected_eof() {
    // Only one byte is available, but a 16-bit read needs two. The reader has hit
    // EOF, so no more bytes can arrive: the shortfall is definitive, as on a slice.
    let mut src = BufSource::new(Chunked {
        data: vec![0xAB],
        pos: 0,
    });
    assert_eq!(
        src.read_bits(16).unwrap_err().kind,
        ErrorKind::UnexpectedEof {
            needed: 16,
            remaining: 8
        }
    );
}

#[test]
fn an_interrupted_read_is_retried() {
    /// Fails the first read with `Interrupted`, as a signal would, then serves `data`.
    struct InterruptedOnce {
        interrupted: bool,
        data: &'static [u8],
    }
    impl std::io::Read for InterruptedOnce {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            self.data.read(buf)
        }
    }
    let mut src = BufSource::new(InterruptedOnce {
        interrupted: false,
        data: &[0xAB, 0xCD],
    });
    assert_eq!(src.read_bits(16).unwrap(), 0xABCD);
    assert_eq!(src.bit_pos(), 16);
}

#[test]
fn an_io_error_from_the_reader_propagates() {
    struct Failing;
    impl std::io::Read for Failing {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "boom",
            ))
        }
    }
    let mut src = BufSource::new(Failing);
    assert!(matches!(
        src.read_bits(8).unwrap_err().kind,
        ErrorKind::Io(_)
    ));
}

#[test]
fn a_full_128_bit_read_is_accepted() {
    // 128 is the widest legal read; only 129 and up are `TooWide`.
    let data: [u8; 16] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ];
    let mut src = BufSource::new(&data[..]);
    // MSB-first, 16 whole bytes: the big-endian integer.
    assert_eq!(src.read_bits(128).unwrap(), u128::from_be_bytes(data));
    assert_eq!(src.bit_pos(), 128);
}

// Returns one byte on the first call, then as many as the buffer takes: a reader
// whose short first read leaves the retention buffer partly filled.
struct ShortThenGreedy<'a> {
    data: &'a [u8],
    first: bool,
}
impl std::io::Read for ShortThenGreedy<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = if self.first { 1 } else { buf.len() }.min(self.data.len());
        self.first = false;
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

#[test]
fn retention_cap_still_bounds_the_buffer_after_a_short_read() {
    // Cap 2, a 3-byte read: after the 1-byte first read only 1 more byte may be
    // retained, however much the reader offers.
    let mut src = BufSource::with_capacity(
        ShortThenGreedy {
            data: &[0xFF; 8],
            first: true,
        },
        2,
    );
    assert_eq!(
        src.read_bits(24).unwrap_err().kind,
        ErrorKind::BufferFull { cap: 2 }
    );
}

#[test]
fn a_read_takes_only_the_bytes_it_needs_from_a_borrowed_reader() {
    // Reading "on demand": after a 3-byte message the borrowed reader must still
    // hold everything behind it, or the bytes of the next message are lost.
    let data = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let mut inner = &data[..];
    let mut src = BufSource::new(&mut inner);
    assert_eq!(src.read_bits(8).unwrap(), 0x11);
    assert_eq!(src.read_bits(16).unwrap(), 0x2233);
    assert_eq!(inner, &data[3..]);
}

#[test]
fn typed_reads_follow_the_configured_layout() {
    use bnb::{BitOrder, ByteOrder, Layout};
    // LSB-first, little-endian is the one layout where dropping *either* order to
    // its default changes a byte-multiple value (see `Source::bit_order`).
    let layout = Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Little,
    };
    let mut src = BufSource::with_capacity_and_layout(&[0x34, 0x12][..], 16, layout);
    assert_eq!(src.bit_order(), BitOrder::Lsb);
    assert_eq!(src.byte_order(), ByteOrder::Little);
    assert_eq!(src.read::<u16>().unwrap(), 0x1234);
}
