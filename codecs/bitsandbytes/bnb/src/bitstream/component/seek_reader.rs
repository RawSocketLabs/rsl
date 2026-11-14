use bnb::{SeekReader, bin, u4};
use std::io::Cursor;

#[test]
fn overflowing_position_is_rejected_before_io() {
    use bnb::{ErrorKind, Source};

    let mut reader = SeekReader::new(Cursor::new([]));
    reader.seek_to_bit(usize::MAX).unwrap();
    let error = reader.read_bits(1).unwrap_err();
    assert_eq!(error.kind, ErrorKind::PositionOverflow);
    assert_eq!(error.at, usize::MAX);
    assert_eq!(reader.bit_pos(), usize::MAX);
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
fn seek_reader_over_a_file_like_source() {
    let wire = vec![0x5A, 0xBC, 0xD0]; // flags=5, value=0xABCD (restore_position layout)
    let mut src = SeekReader::new(Cursor::new(wire));
    let f = Frame::decode(&mut src).unwrap();
    assert_eq!(f.value, 0xABCD);
    assert_eq!(f.peek, 0xAB, "rewound and re-read via io::Seek");
}

#[test]
fn over_wide_read_is_rejected() {
    use bnb::{ErrorKind, Source};
    let mut src = SeekReader::new(Cursor::new(vec![0u8; 32]));
    assert!(matches!(
        src.read_bits(129).unwrap_err().kind,
        ErrorKind::TooWide { width: 129 }
    ));
}

#[test]
fn reading_past_the_end_is_unexpected_eof() {
    use bnb::ErrorKind;
    #[bin(big)]
    #[derive(Debug)]
    struct Quad {
        v: u32,
    }
    // Only two of the four needed bytes are present.
    let mut src = SeekReader::new(Cursor::new(vec![0x12, 0x34]));
    assert!(matches!(
        Quad::decode(&mut src).unwrap_err().kind,
        ErrorKind::UnexpectedEof { .. }
    ));
}

#[test]
fn little_endian_layout_is_honored() {
    #[bin(little)]
    #[derive(Debug, PartialEq)]
    struct Le {
        v: u32,
    }
    // `with_layout` carries the message's little-endian order onto the reader.
    let mut src = SeekReader::with_layout(
        Cursor::new(vec![0x78, 0x56, 0x34, 0x12]),
        <Le as bnb::BitEncode>::LAYOUT,
    );
    assert_eq!(Le::decode(&mut src).unwrap(), Le { v: 0x1234_5678 });
}

/// Guards the shared-cursor contract: every `&File` moves one OS offset, so a
/// `SeekReader` must not trust a remembered position. Two readers (one a clone)
/// interleave reads at different offsets and each still sees its own bytes.
#[test]
fn interleaved_readers_over_one_shared_file_each_read_their_own_bytes() {
    use bnb::Source;
    use std::io::Write;

    let path = std::env::temp_dir().join(format!(
        "bnb-shared-cursor-{}-{:?}.bin",
        std::process::id(),
        std::thread::current().id()
    ));
    let data: Vec<u8> = (0..=255).collect();
    std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(&data))
        .unwrap();
    let file = std::fs::File::open(&path).unwrap();

    let mut low = SeekReader::new(&file);
    let mut high = low.clone();
    high.seek_to_bit(128 * 8).unwrap();
    for i in 0..64u16 {
        let lo = low.read::<u16>().unwrap();
        let hi = high.read::<u16>().unwrap();
        let at = usize::from(i) * 2;
        assert_eq!(
            lo,
            u16::from_be_bytes([data[at], data[at + 1]]),
            "low reader, word {i}"
        );
        assert_eq!(
            hi,
            u16::from_be_bytes([data[128 + at], data[129 + at]]),
            "high reader, word {i}"
        );
    }
    drop(file);
    std::fs::remove_file(&path).unwrap();
}
