use bnb::{BufSeekReader, ErrorKind, SeekReader, Source};
use std::io::{Cursor, Read, Seek, SeekFrom};

/// The fixture byte at offset `i`: a period (251) that no power-of-two stride aliases.
fn byte_at(i: usize) -> u8 {
    u8::try_from(i % 251).expect("below 251")
}

/// Counts the calls that reach the underlying reader.
struct Counting {
    inner: Cursor<Vec<u8>>,
    seeks: usize,
    reads: usize,
    /// The largest buffer a `read` asked to fill (the `BufReader` capacity).
    widest_read: usize,
}

impl Counting {
    /// A `len`-byte [`byte_at`] fixture with zeroed counters.
    fn new(len: usize) -> Self {
        Self {
            inner: Cursor::new((0..len).map(byte_at).collect()),
            seeks: 0,
            reads: 0,
            widest_read: 0,
        }
    }
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        self.widest_read = self.widest_read.max(buf.len());
        self.inner.read(buf)
    }
}

impl Seek for Counting {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.seeks += 1;
        self.inner.seek(pos)
    }
}

#[test]
fn sequential_reads_seek_once_and_read_once_per_buffer() {
    const LEN: usize = 64 * 1024;
    let mut src = BufSeekReader::new(Counting::new(LEN));
    for i in 0..LEN / 2 {
        let at = i * 2;
        let want = u16::from_be_bytes([byte_at(at), byte_at(at + 1)]);
        assert_eq!(src.read::<u16>().unwrap(), want, "word {i}");
    }
    let inner = src.get_ref();
    assert_eq!(inner.seeks, 1, "only the initial absolute seek");
    assert!(
        inner.reads <= LEN.div_ceil(inner.widest_read) + 1,
        "one underlying read per buffer fill (capacity {}), got {}",
        inner.widest_read,
        inner.reads
    );
}

#[test]
fn a_nearby_backward_read_is_served_from_the_buffer() {
    let mut src = BufSeekReader::new(Counting::new(4096));
    src.read::<u8>().unwrap(); // buffers from byte 0
    let before = (src.get_ref().seeks, src.get_ref().reads);
    src.seek_to_bit(100 * 8).unwrap();
    assert_eq!(src.read::<u32>().unwrap(), 0x6465_6667);
    src.seek_to_bit(50 * 8).unwrap();
    assert_eq!(src.read::<u8>().unwrap(), 50);
    let inner = src.get_ref();
    assert_eq!(
        (inner.seeks, inner.reads),
        before,
        "no I/O for buffered bytes"
    );
}

#[test]
fn a_read_beyond_the_buffer_seeks_and_stays_correct() {
    let mut src = BufSeekReader::new(Counting::new(64 * 1024));
    src.read::<u8>().unwrap();
    src.seek_to_bit(40_000 * 8).unwrap();
    assert_eq!(src.read::<u8>().unwrap(), byte_at(40_000));
    src.seek_to_bit(3 * 8).unwrap();
    assert_eq!(src.read::<u8>().unwrap(), 3);
    assert_eq!(
        src.get_ref().seeks,
        3,
        "one seek per jump out of the buffer"
    );
}

/// Fails its first `read` or `seek` call (per flag), then behaves.
struct FailOnce {
    inner: Cursor<Vec<u8>>,
    fail_read: bool,
    fail_seek: bool,
}

impl Read for FailOnce {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if std::mem::take(&mut self.fail_read) {
            return Err(std::io::ErrorKind::ConnectionReset.into());
        }
        self.inner.read(buf)
    }
}

impl Seek for FailOnce {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        if std::mem::take(&mut self.fail_seek) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        self.inner.seek(pos)
    }
}

/// A four-byte `FailOnce` fixture (`12 34 56 78`).
fn fail_once(fail_read: bool, fail_seek: bool) -> FailOnce {
    FailOnce {
        inner: Cursor::new(vec![0x12, 0x34, 0x56, 0x78]),
        fail_read,
        fail_seek,
    }
}

#[test]
fn read_and_seek_failures_match_seek_reader_and_then_recover() {
    for (fail_read, fail_seek, kind) in [
        (true, false, std::io::ErrorKind::ConnectionReset),
        (false, true, std::io::ErrorKind::PermissionDenied),
    ] {
        let mut reference = SeekReader::new(fail_once(fail_read, fail_seek));
        let mut candidate = BufSeekReader::new(fail_once(fail_read, fail_seek));
        reference.seek_to_bit(4).unwrap();
        candidate.seek_to_bit(4).unwrap();
        let expected = reference.read_bits(16).unwrap_err();
        assert_eq!(candidate.read_bits(16).unwrap_err(), expected);
        assert_eq!(expected.kind, ErrorKind::Io(kind));
        assert_eq!(candidate.bit_pos(), 4, "a failed read does not advance");
        assert_eq!(
            candidate.read_bits(16).unwrap(),
            0x2345,
            "the retry re-seeks instead of trusting a stale offset"
        );
    }
}

#[test]
fn end_of_input_and_argument_errors_match_seek_reader() {
    let bytes = vec![0x12, 0x34];
    for (start, width) in [
        (0, 32),
        (12, 8),
        (16, 1),
        (100, 0),
        (0, 129),
        (usize::MAX, 1),
    ] {
        let mut reference = SeekReader::new(Cursor::new(bytes.clone()));
        let mut candidate = BufSeekReader::new(Cursor::new(bytes.clone()));
        reference.seek_to_bit(start).unwrap();
        candidate.seek_to_bit(start).unwrap();
        assert_eq!(
            candidate.read_bits(width),
            reference.read_bits(width),
            "read of {width} bits at bit {start}"
        );
        assert_eq!(candidate.bit_pos(), reference.bit_pos());
    }
}
