//! `restore_position` + `SeekSource`: `#[br(restore_position)]`
//! reads a field (a peek), then rewinds so a later field re-reads the same bytes.
//! It needs a seekable source (the slice `BitReader`); on a forward stream it errors
//! (`ErrorKind::NotSeekable`), and `#[bin(forward_only)]` makes it a compile error.

mod macro_ {

    use bnb::{bin, u4};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        flags: u4,
        #[br(restore_position)]
        peek: u8, // peek the byte at the value's offset...
        value: u16, // ...then read the full value from the same offset
    }

    #[test]
    fn restore_position_peeks_then_rereads() {
        let f = Frame {
            flags: u4::new(0x5),
            peek: 0xAB, // the high byte of `value`
            value: 0xABCD,
        };
        let bytes = f.to_bytes().unwrap();
        // flags(0101) | value(0xABCD, 16 bits) — `peek` overlaps `value`, not written.
        assert_eq!(bytes, [0x5A, 0xBC, 0xD0]);

        let decoded = Frame::decode_exact(&bytes).unwrap();
        assert_eq!(decoded.value, 0xABCD);
        assert_eq!(decoded.peek, 0xAB, "peeked the high byte at value's offset");
        assert_eq!(decoded, f);
    }

    /// A stored peek kept consistent by `#[bw(calc = …)]`: never written (the field it
    /// overlaps owns the bytes), but recomputed by the canonical helpers.
    #[bin(big)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct CalcPeek {
        #[br(restore_position)]
        #[bw(calc = self.value.to_be_bytes()[0])]
        peek: u8,
        value: u16,
    }

    #[test]
    fn stored_peek_with_calc_round_trips_and_canonicalizes() {
        let decoded = CalcPeek::decode_exact(&[0xAB, 0xCD]).unwrap();
        let fresh = CalcPeek {
            peek: 0xAB,
            value: 0xABCD,
        };
        assert_eq!(decoded, fresh);
        assert_eq!(decoded.to_bytes().unwrap(), [0xAB, 0xCD]);
        assert!(decoded.is_canonical());

        let stale = CalcPeek {
            peek: 0,
            value: 0xABCD,
        };
        assert_eq!(
            stale.to_bytes().unwrap(),
            [0xAB, 0xCD],
            "a peek is never written, stale or not"
        );
        assert_eq!(stale.to_canonical_bytes().unwrap(), [0xAB, 0xCD]);
        assert_eq!(stale.canonical_diff(), ["peek"]);
        assert_eq!(stale.to_canonical(), fresh);
    }
}
