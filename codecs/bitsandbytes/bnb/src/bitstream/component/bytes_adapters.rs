use crate::bitstream::source::read_by_bits;
use bnb::{
    BitEncode, BitOrder, ByteOrder, BytesReader, BytesWriter, Layout, Source, bin, u4, u12, u24,
};

#[test]
fn a_bytes_reader_lends_the_whole_bytes_it_holds_and_moves_past_them() {
    let mut reader = BytesReader::new(bytes::Bytes::from_static(&[1, 2, 3, 4, 5]));
    assert_eq!(reader.aligned_run(2), Some(&[1, 2][..]));
    assert_eq!(reader.bit_pos(), 16);
    assert_eq!(reader.aligned_run(4), None);
    reader.read_bits(4).unwrap();
    assert_eq!(reader.aligned_run(1), None);
    assert_eq!(reader.bit_pos(), 20);
}

#[test]
fn a_typed_read_is_the_read_taken_bit_by_bit_at_any_cursor() {
    let wire = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF1, 0x23];
    for bit in [BitOrder::Msb, BitOrder::Lsb] {
        for byte in [ByteOrder::Big, ByteOrder::Little] {
            let layout = Layout { bit, byte };
            // Past the end too: the reader may seek anywhere, and a read from
            // there is the seek's error.
            for pos in 0..=wire.len() * 8 + 9 {
                let at = || {
                    let mut reader = BytesReader::with_layout(wire.to_vec().into(), layout);
                    reader.seek_to_bit(pos).unwrap();
                    reader
                };
                macro_rules! each {
                    ($($value:ty),*) => {$(
                        let (mut fast, mut slow) = (at(), at());
                        let got = fast.read::<$value>().map_err(|e| (e.kind, e.at));
                        let expected = read_by_bits::<_, $value>(&mut slow)
                            .map_err(|e| (e.kind, e.at));
                        assert_eq!(got, expected, "{layout:?} pos {pos}");
                        assert_eq!(fast.bit_pos(), slow.bit_pos(), "{layout:?} pos {pos}");
                    )*};
                }
                each!(u4, u8, u12, u16, u24, u32, u64, u128);
            }
        }
    }
}

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
