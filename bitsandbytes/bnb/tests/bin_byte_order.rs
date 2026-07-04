//! `big`/`little` byte order (ROADMAP Phase 2). Byte order is applied to
//! byte-multiple field values (binrw applies it only to byte-multiple types); the
//! default is big-endian (network order). Sub-byte fields are unaffected.

mod macro_ {

    use bnb::{bin, u4, u12};

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct BeWord {
        value: u16,
        big: u32,
    }

    #[bin(little)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct LeWord {
        value: u16,
        big: u32,
    }

    #[test]
    fn byte_order_visible_on_the_wire() {
        let be = BeWord {
            value: 0x1234,
            big: 0xAABB_CCDD,
        };
        assert_eq!(be.to_bytes().unwrap(), [0x12, 0x34, 0xAA, 0xBB, 0xCC, 0xDD]);

        let le = LeWord {
            value: 0x1234,
            big: 0xAABB_CCDD,
        };
        assert_eq!(le.to_bytes().unwrap(), [0x34, 0x12, 0xDD, 0xCC, 0xBB, 0xAA]);

        assert_eq!(BeWord::decode_exact(&be.to_bytes().unwrap()).unwrap(), be);
        assert_eq!(LeWord::decode_exact(&le.to_bytes().unwrap()).unwrap(), le);
    }

    // Little-endian with a sub-byte lead field (so multi-byte values straddle bytes).
    #[bin(little)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct LeStraddle {
        tag: u4,
        value: u16,
        word: u32,
    }

    #[test]
    fn little_endian_straddling_round_trips() {
        let f = LeStraddle {
            tag: u4::new(0x5),
            value: 0x1234,
            word: 0xAABB_CCDD,
        };
        assert_eq!(LeStraddle::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }

    // Sub-byte (non-byte-multiple) widths are not byte-swapped, regardless of order.
    #[bin(little)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct SubByte {
        a: u4,
        b: u12,
    }

    #[test]
    fn sub_byte_fields_unaffected() {
        let f = SubByte {
            a: u4::new(0xA),
            b: u12::new(0x123),
        };
        assert_eq!(SubByte::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }
}
