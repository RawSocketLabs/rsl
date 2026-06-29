//! `bytes` integration (ROADMAP Phase 3, the `bytes` feature): zero-copy
//! `BytesReader`/`BytesWriter`. Decode from an owned `Bytes` frame; encode into a
//! writer you `freeze()` to a `Bytes` — the async/tokio framing path.
#![cfg(feature = "bytes")]

mod component {

    use bnb::{BitEncode, BytesReader, BytesWriter, bin, u4, u12};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        a: u4,
        b: u12,
    }

    #[test]
    fn round_trip_through_bytes() {
        let f = Frame {
            a: u4::new(0xA),
            b: u12::new(0x123),
        };

        // Encode into a BytesWriter, then freeze to a zero-copy Bytes.
        let mut w = BytesWriter::new();
        f.bit_encode(&mut w).unwrap();
        let frozen = w.freeze();
        assert_eq!(&frozen[..], &[0xA1, 0x23]);

        // Decode from an owned Bytes via BytesReader.
        let mut r = BytesReader::new(frozen.clone());
        let decoded = Frame::decode(&mut r).unwrap();
        assert_eq!(decoded, f);
    }

    // A `restore_position` message decodes over `BytesReader` (a `SeekSource`), exercising its
    // `bit_pos`/`seek_to_bit`. The frame is produced via `BytesWriter::freeze` (no `bytes::` name).
    #[test]
    fn bytes_reader_seek_and_bit_pos() {
        use bnb::{Sink, Source};
        #[bin(big)]
        #[derive(Debug, PartialEq, Eq)]
        struct Peeked {
            #[br(restore_position)]
            tag: u8,
            full: u16,
        }
        let mut w = BytesWriter::new();
        w.write(0xABu8).unwrap();
        w.write(0xCDu8).unwrap();
        let mut r = BytesReader::new(w.freeze());
        assert_eq!(r.bit_pos(), 0);
        let p = Peeked::decode(&mut r).unwrap();
        assert_eq!((p.tag, p.full), (0xAB, 0xABCD));
    }

    #[test]
    fn bytes_writer_with_layout_and_bit_pos() {
        use bnb::{BitOrder, ByteOrder, Layout, Sink};
        let mut w = BytesWriter::with_layout(Layout {
            bit: BitOrder::Lsb,
            byte: ByteOrder::Big,
        });
        assert_eq!(w.bit_pos(), 0);
        w.write(u4::new(0xA)).unwrap();
        assert_eq!(w.bit_pos(), 4);
    }
}
