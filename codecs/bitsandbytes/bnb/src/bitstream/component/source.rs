use bnb::{BitDecode, BitEncode, BitReader, BitWriter, StreamBitReader, u4, u12};

#[derive(BitDecode, BitEncode, Debug, PartialEq, Eq)]
struct Word {
    a: u4,
    b: u12, // 16 bits; all <= 64 so the streaming reader handles it too
}

#[test]
fn decodes_over_slice_and_stream_identically() {
    let word = Word {
        a: u4::new(0xA),
        b: u12::new(0xBCD),
    };
    let mut w = BitWriter::new();
    word.bit_encode(&mut w).unwrap();
    let bytes = w.into_bytes();
    assert_eq!(bytes, [0xAB, 0xCD]);

    // Source 1 — in-memory slice cursor (random-access, full power).
    let mut slice = BitReader::new(&bytes);
    assert_eq!(Word::bit_decode(&mut slice).unwrap(), word);

    // Source 2 — a forward `Read` (`&[u8]` is `Read` but NOT `Seek`); same code,
    // no rewrite, no Seek requirement.
    let mut stream = StreamBitReader::new(&bytes[..]);
    assert_eq!(Word::bit_decode(&mut stream).unwrap(), word);
}

#[test]
fn stream_reader_honors_a_little_endian_layout() {
    use bnb::{Layout, bin};
    #[bin(little)]
    #[derive(Debug, PartialEq)]
    struct Le {
        v: u32,
    }
    let bytes = Le { v: 0x1122_3344 }.to_bytes().unwrap();
    assert_eq!(bytes, [0x44, 0x33, 0x22, 0x11]); // little-endian on the wire

    // A default stream reader would misread it big-endian; with the type's layout
    // it decodes correctly over a forward `Read`.
    let mut stream = StreamBitReader::with_layout(&bytes[..], <Le as BitEncode>::LAYOUT);
    assert_eq!(Le::bit_decode(&mut stream).unwrap(), Le { v: 0x1122_3344 });
    // (The default `new` reader, big-endian, would read the bytes reversed.)
    let mut msb = StreamBitReader::with_layout(&bytes[..], Layout::default());
    assert_ne!(Le::bit_decode(&mut msb).unwrap().v, 0x1122_3344);
}

#[test]
fn lsb_sub_byte_reads_carry_the_unread_high_bits_across_a_byte_boundary() {
    use bnb::{BitOrder, ByteOrder, Layout, Source};
    // Every byte has set high bits, so a dropped or misplaced carry changes a value.
    let bytes = [0xB7, 0x6C, 0xE5, 0x9A, 0xD3];
    let layout = Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Little,
    };
    let mut stream = StreamBitReader::with_layout(&bytes[..], layout);
    let mut slice = BitReader::with_layout(&bytes, layout);
    // By hand, LSB-first: 0xB7 = 1011_0111 -> low 3 bits = 111; then its 5 high bits
    // (10110) are the low end of the next value, under the low 2 bits of 0x6C (00).
    assert_eq!(stream.read_bits(3).unwrap(), 0b111);
    assert_eq!(stream.read_bits(7).unwrap(), 0b00_10110);
    slice.read_bits(10).unwrap();
    // The rest against the slice reader, splitting bytes at varying offsets.
    for n in [5u32, 9, 4, 12] {
        assert_eq!(
            stream.read_bits(n).unwrap(),
            slice.read_bits(n).unwrap(),
            "read_bits({n}) diverged from the slice reader"
        );
        assert_eq!(Source::bit_pos(&stream), slice.bit_pos());
    }
}
