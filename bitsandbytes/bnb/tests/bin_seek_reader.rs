//! `SeekReader` (ROADMAP Phase 3b): a `SeekSource` over a `Read + Seek` (a file-like)
//! that seeks via `io::Seek` to the byte holding the bit cursor, with no buffering —
//! the large-file / container-format case. A seek-using message round-trips over it.

mod component {

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
}
