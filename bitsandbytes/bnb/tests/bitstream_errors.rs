//! Position-aware errors (ROADMAP Phase 1): a decode/encode failure reports the
//! bit offset and the field being processed — the runtime analogue of binrw's
//! error spans.

mod component {

    use bnb::{BitDecode, BitEncode, BitError, BitReader, BitWriter, ErrorKind, u4, u12};

    #[derive(BitDecode, BitEncode, Debug, PartialEq, Eq)]
    struct Header {
        a: u4,
        b: u12, // a + b = 16 bits
    }

    #[test]
    fn round_trips() {
        let h = Header {
            a: u4::new(0xA),
            b: u12::new(0xBCD),
        };
        let mut w = BitWriter::new();
        h.bit_encode(&mut w).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xAB, 0xCD]);

        let mut r = BitReader::new(&bytes);
        assert_eq!(Header::bit_decode(&mut r).unwrap(), h);
    }

    #[test]
    fn decode_eof_reports_offset_and_field() {
        // One byte: `a` (4 bits) decodes; `b` (12 bits) runs off the end at bit 4.
        let bytes = [0xAB];
        let mut r = BitReader::new(&bytes);
        let err: BitError = Header::bit_decode(&mut r).unwrap_err();

        assert_eq!(err.field, Some("b"), "names the field that failed");
        assert_eq!(err.at, 4, "records the bit offset where decoding stopped");
        assert_eq!(
            err.kind,
            ErrorKind::UnexpectedEof {
                needed: 12,
                remaining: 4
            }
        );

        let msg = err.to_string();
        assert!(msg.contains("field `b`"), "message names the field: {msg}");
        assert!(msg.contains("at bit 4"), "message names the offset: {msg}");
    }

    #[test]
    fn innermost_field_wins_the_span() {
        // The error originates in `b`'s read; the outer struct must not overwrite it.
        let mut r = BitReader::new(&[0xAB]);
        let err = Header::bit_decode(&mut r).unwrap_err();
        assert_eq!(err.field, Some("b"));
    }
}
