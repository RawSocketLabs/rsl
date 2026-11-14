use bnb::{BitDecode, BitEncode, BitReader, EncodeExt, ErrorKind, StreamBitReader, u4, u12};
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
fn decode_advances_a_cursor() {
    let (w, bytes) = sample();
    let mut both = bytes.to_vec();
    both.extend_from_slice(&bytes); // two messages back to back

    let mut cur = BitReader::new(&both);
    assert_eq!(Word::decode(&mut cur).unwrap(), w);
    assert_eq!(cur.bit_pos(), 16, "advanced past the first message");
    assert_eq!(Word::decode(&mut cur).unwrap(), w);
    assert_eq!(cur.bit_pos(), both.len() * 8, "consumed both");
}

#[test]
fn decode_all_and_iter_collect_back_to_back() {
    let (w, bytes) = sample();
    let mut both = bytes.to_vec();
    both.extend_from_slice(&bytes);

    // decode_all — eager; decode_iter — lazy. Both layout-baked and bit-aware.
    assert_eq!(Word::decode_all(&both).unwrap(), vec![w, w]);
    let collected: Result<Vec<_>, _> = Word::decode_iter(&both).collect();
    assert_eq!(collected.unwrap(), vec![w, w]);
}

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct Twelve {
    v: u12,
}

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
struct Nibble {
    v: u4,
}

#[test]
fn decode_all_and_iter_treat_an_undecodable_sub_byte_tail_as_padding() {
    let t = Twelve { v: u12::new(0xABC) };
    let bytes = t.to_bytes().unwrap();
    assert_eq!(bytes, [0xAB, 0xC0]);
    assert_eq!(Twelve::decode_exact(&bytes).unwrap(), t);
    for wire in [bytes, vec![0xAB, 0xCF]] {
        assert_eq!(Twelve::decode_all(&wire).unwrap(), vec![t]);
        let collected: Result<Vec<_>, _> = Twelve::decode_iter(&wire).collect();
        assert_eq!(collected.unwrap(), vec![t]);
    }

    // A whole trailing byte is not padding: a truncated message still errors.
    let wire = [0xAB, 0xCA, 0xBC, 0x00];
    assert!(matches!(
        Twelve::decode_all(&wire).unwrap_err().kind,
        ErrorKind::UnexpectedEof { .. }
    ));
    let last = Twelve::decode_iter(&wire).last().unwrap();
    assert!(matches!(
        last.unwrap_err().kind,
        ErrorKind::UnexpectedEof { .. }
    ));
}

#[test]
fn decode_all_keeps_a_sub_byte_message_in_the_final_byte() {
    let nibbles = [Nibble { v: u4::new(0xA) }, Nibble { v: u4::new(0xB) }];
    assert_eq!(Nibble::decode_all(&[0xAB]).unwrap(), nibbles);
    let collected: Result<Vec<_>, _> = Nibble::decode_iter(&[0xAB]).collect();
    assert_eq!(collected.unwrap(), nibbles);
}

#[test]
fn decode_all_and_iter_of_a_zero_width_type_are_no_progress() {
    use bnb::{BitDecode, BitError, Layout, Source};
    #[derive(Debug, PartialEq)]
    struct Zero;
    impl BitDecode for Zero {
        fn bit_decode<S: Source>(_r: &mut S) -> Result<Self, BitError> {
            Ok(Zero)
        }
    }
    // A zero-width type makes no progress; over a non-empty buffer both entry points
    // must say so once (no spurious element, no spinning) rather than drop the bytes.
    let error = bnb::__private::decode_all::<Zero>(&[0xFF, 0xFF], Layout::default()).unwrap_err();
    assert_eq!((error.kind, error.at), (ErrorKind::NoProgress, 0));
    let mut items = bnb::__private::decode_iter::<Zero>(&[0xFF, 0xFF], Layout::default());
    let error = items.next().unwrap().unwrap_err();
    assert_eq!((error.kind, error.at), (ErrorKind::NoProgress, 0));
    assert_eq!(items.next(), None);
}

#[test]
fn decode_errors_on_short_cursor() {
    let short = [0xABu8]; // one byte; Word needs two
    let mut cur = BitReader::new(&short);
    let err = Word::decode(&mut cur).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
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
fn decode_explicit_cursor() {
    let (w, bytes) = sample();
    let mut r = BitReader::new(&bytes);
    assert_eq!(Word::decode(&mut r).unwrap(), w);
}

#[test]
fn streaming_shortfall_is_incomplete_not_eof() {
    let (_, bytes) = sample();
    // Only the first byte available over a stream: the shortfall is the retry
    // signal, not a definitive EOF.
    let mut stream = StreamBitReader::new(&bytes[..1]);
    let err = Word::decode(&mut stream).unwrap_err();
    assert!(err.is_incomplete(), "stream shortfall is incomplete: {err}");
    assert!(matches!(err.kind, ErrorKind::Incomplete { .. }));
    assert_eq!(err.field, Some("b"), "still records the field span");
}
