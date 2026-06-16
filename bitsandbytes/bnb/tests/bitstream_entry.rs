//! Entry points (ROADMAP Phase 1, chunk B2): `decode`/`peek`/`decode_exact`/
//! `decode_from` + `encode`/`to_bytes`/`encode_into`, with the `Incomplete`
//! (streaming) and `TrailingBytes` (strict) signals.

use bnb::{BitDecode, BitEncode, BitReader, ErrorKind, StreamBitReader, u4, u12};
use std::io::Cursor;

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct Word {
    a: u4,
    b: u12,
}

fn sample() -> (Word, [u8; 2]) {
    (
        Word {
            a: u4::new(0xA),
            b: u12::new(0xBCD),
        },
        [0xAB, 0xCD],
    )
}

#[test]
fn to_bytes_peek_and_tail_tolerance() {
    let (w, bytes) = sample();
    assert_eq!(w.to_bytes().unwrap(), bytes);
    assert_eq!(Word::peek(&bytes).unwrap(), w);

    // peek is tail-tolerant: a trailing byte is ignored.
    let mut padded = bytes.to_vec();
    padded.push(0xFF);
    assert_eq!(Word::peek(&padded).unwrap(), w);
}

#[test]
fn decode_consumes_and_advances() {
    let (w, bytes) = sample();
    let mut both = bytes.to_vec();
    both.extend_from_slice(&bytes); // two messages back to back

    let mut cursor: &[u8] = &both;
    assert_eq!(Word::decode(&mut cursor).unwrap(), w);
    assert_eq!(cursor.len(), 2, "advanced past the first message");
    assert_eq!(Word::decode(&mut cursor).unwrap(), w);
    assert!(cursor.is_empty(), "consumed both");
}

#[test]
fn decode_is_transactional_on_error() {
    let short = [0xABu8]; // one byte; Word needs two
    let mut cursor: &[u8] = &short;
    let err = Word::decode(&mut cursor).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    assert_eq!(cursor.len(), 1, "buffer restored on error (transactional)");
}

#[test]
fn decode_exact_rejects_trailing_bytes() {
    let (w, bytes) = sample();
    assert_eq!(Word::decode_exact(&bytes).unwrap(), w);

    let mut padded = bytes.to_vec();
    padded.push(0xFF);
    let err = Word::decode_exact(&padded).unwrap_err();
    assert_eq!(err.kind, ErrorKind::TrailingBytes { remaining: 1 });
}

#[test]
fn encode_to_any_write() {
    let (w, bytes) = sample();
    let mut sink = Cursor::new(Vec::new());
    w.encode(&mut sink).unwrap();
    assert_eq!(sink.into_inner(), bytes);
}

#[test]
fn encode_io_error_is_reported() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (w, _) = sample();
    let err = w.encode(&mut Full).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Io(std::io::ErrorKind::WriteZero));
}

#[test]
fn decode_from_explicit_cursor() {
    let (w, bytes) = sample();
    let mut r = BitReader::new(&bytes);
    assert_eq!(Word::decode_from(&mut r).unwrap(), w);
}

#[test]
fn streaming_shortfall_is_incomplete_not_eof() {
    let (_, bytes) = sample();
    // Only the first byte available over a stream: the shortfall is the retry
    // signal, not a definitive EOF.
    let mut stream = StreamBitReader::new(&bytes[..1]);
    let err = Word::decode_from(&mut stream).unwrap_err();
    assert!(err.is_incomplete(), "stream shortfall is incomplete: {err}");
    assert!(matches!(err.kind, ErrorKind::Incomplete { .. }));
    assert_eq!(err.field, Some("b"), "still records the field span");
}
