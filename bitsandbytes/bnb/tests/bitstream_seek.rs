//! Spike (DESIGN §11): seeking is free on the in-memory cursor, and a
//! forward-only reader needs only `Read` — no `Seek`, no `NoSeek` wrapper.

mod component {

    use bnb::{BitReader, StreamBitReader, u4};

    #[test]
    fn seek_and_align_need_no_seek_trait() {
        // 3 bytes: 0xAB, 0xCD, 0xEF.
        let bytes = [0xABu8, 0xCD, 0xEF];
        let mut r = BitReader::new(&bytes);

        // Read a nibble, jump to an absolute bit offset, read, then jump back —
        // exactly the move DNS name-compression needs, with no Seek machinery.
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
        r.seek_to_bit(16).unwrap(); // -> third byte
        assert_eq!(r.read_bits(8).unwrap(), 0xEF);
        r.seek_to_bit(4).unwrap(); // back to the low nibble of byte 0
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xB));

        // align_to_byte snaps the cursor forward to the next byte boundary.
        r.seek_to_bit(9).unwrap();
        r.align_to_byte();
        assert_eq!(r.bit_pos(), 16);

        // Seeking past the end is a clean error, not a panic.
        assert!(r.seek_to_bit(999).is_err());
    }

    #[test]
    fn forward_only_stream_reader_requires_only_read() {
        // `&[u8]` implements `std::io::Read` but NOT `std::io::Seek`. That this
        // compiles and runs is the whole point: forward bit parsing drops the Seek
        // requirement binrw imposes uniformly.
        let data = [0xABu8, 0xCD];
        let src: &[u8] = &data;
        let mut r = StreamBitReader::new(src);

        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xB));
        assert_eq!(r.read_bits(8).unwrap(), 0xCD);
        // Past the end -> error, not panic.
        assert!(r.read_bits(1).is_err());
    }
}
