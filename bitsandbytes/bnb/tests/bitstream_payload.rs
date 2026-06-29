//! Fixed payload fields (ROADMAP Phase 1, chunk E): a `[u8; N]` byte-array field
//! (read/written even at a non-byte-aligned offset). Variable-length `Vec` +
//! `count` is Phase 2.

mod macro_ {

    use bnb::{BitDecode, BitEncode, BitReader, BitWriter, ErrorKind, FixedBitLen, u4, u12};

    #[derive(BitDecode, BitEncode, Debug, PartialEq, Eq, Clone, Copy)]
    struct Frame {
        tag: u4,
        payload: [u8; 4], // a fixed 32-bit payload, starting at bit 4 (straddles bytes)
        crc: u12,         // 4 + 32 + 12 = 48 bits
    }

    fn sample() -> Frame {
        Frame {
            tag: u4::new(0x5),
            payload: [0xDE, 0xAD, 0xBE, 0xEF],
            crc: u12::new(0xABC),
        }
    }

    #[test]
    fn fixed_byte_array_round_trips() {
        let f = sample();
        let mut w = BitWriter::new();
        f.bit_encode(&mut w).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 6, "48 bits");

        let mut r = BitReader::new(&bytes);
        assert_eq!(Frame::bit_decode(&mut r).unwrap(), f);
        // ...and through the high-level entry points.
        assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }

    #[test]
    fn payload_counts_n_times_8_in_bit_len() {
        assert_eq!(<Frame as FixedBitLen>::BIT_LEN, 4 + 32 + 12);
    }

    #[test]
    fn payload_eof_names_the_field() {
        // tag(4) decodes, then payload needs 32 bits but only 4 remain.
        let short = [0x50];
        let mut r = BitReader::new(&short);
        let err = Frame::bit_decode(&mut r).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
        assert_eq!(err.field, Some("payload"));
    }
}
