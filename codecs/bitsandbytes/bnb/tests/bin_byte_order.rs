//! `big`/`little` byte order (ROADMAP Phase 2). Byte order is applied to
//! byte-multiple field values (binrw applies it only to byte-multiple types); the
//! default is big-endian (network order). Sub-byte fields are unaffected.

mod macro_ {

    use bnb::{bin, bitfield, bitflags, u4, u12};

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

    // A nested `#[bitfield]`/`#[bitflags]` keeps its own declared byte order, so its bytes
    // inside a message are the same as its standalone `to_*_bytes`.
    #[bitfield(u16, bytes = little)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct LeReg {
        hi: u8,
        lo: u8,
    }

    #[bitfield(u16, bytes = big)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct BeReg {
        hi: u8,
        lo: u8,
    }

    #[bitflags(u16, bytes = little)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct LeFlags {
        a: bool,
        #[flag(9)]
        b: bool,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct BigCarriesLittle {
        tag: u8,
        reg: LeReg,
        flags: LeFlags,
    }

    #[bin(little)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct LittleCarriesBig {
        reg: BeReg,
        word: u16,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct StraddledLittle {
        tag: u4,
        reg: LeReg,
        pad: u4,
    }

    #[test]
    fn nested_bitfield_keeps_its_declared_byte_order() {
        let reg = LeReg::from_raw(0xABCD);
        assert_eq!(reg.to_le_bytes(), [0xCD, 0xAB]);
        let flags = LeFlags::A | LeFlags::B;
        let m = BigCarriesLittle {
            tag: 0x01,
            reg,
            flags,
        };
        let wire = m.to_bytes().unwrap();
        assert_eq!(wire, [0x01, 0xCD, 0xAB, 0x01, 0x02]);
        assert_eq!(BigCarriesLittle::decode_exact(&wire).unwrap(), m);

        let m = LittleCarriesBig {
            reg: BeReg::from_raw(0xABCD),
            word: 0x1234,
        };
        let wire = m.to_bytes().unwrap();
        assert_eq!(wire, [0xAB, 0xCD, 0x34, 0x12]);
        assert_eq!(LittleCarriesBig::decode_exact(&wire).unwrap(), m);

        // At a sub-byte offset the swap still applies to the value's 16 bits.
        let m = StraddledLittle {
            tag: u4::new(0xF),
            reg,
            pad: u4::new(0),
        };
        let wire = m.to_bytes().unwrap();
        assert_eq!(wire, [0xFC, 0xDA, 0xB0]);
        assert_eq!(StraddledLittle::decode_exact(&wire).unwrap(), m);
    }

    // The declared order holds however the message reaches the value, not only as a plain
    // field: a `map` reads the bitfield through the same cursor.
    #[bin(big)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct MappedLittle {
        #[br(map = |r: LeReg| r.to_raw())]
        #[bw(map = |v: &u16| LeReg::from_raw(*v))]
        raw: u16,
    }

    // An LSB-first little-endian message carrying a big-endian register.
    #[bin(little, bits = lsb)]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct LsbCarriesBig {
        reg: BeReg,
        word: u16,
    }

    #[test]
    fn declared_byte_order_holds_through_map_and_lsb_messages() {
        let m = MappedLittle { raw: 0xABCD };
        let wire = m.to_bytes().unwrap();
        assert_eq!(wire, [0xCD, 0xAB]);
        assert_eq!(MappedLittle::decode_exact(&wire).unwrap(), m);
        assert_eq!(
            MappedLittle::decode_exact(&wire).unwrap().raw,
            BigCarriesLittle::decode_exact(&[0x00, 0xCD, 0xAB, 0x00, 0x00])
                .unwrap()
                .reg
                .to_raw()
        );

        let m = LsbCarriesBig {
            reg: BeReg::from_raw(0xABCD),
            word: 0x1234,
        };
        let wire = m.to_bytes().unwrap();
        assert_eq!(wire, [0xAB, 0xCD, 0x34, 0x12]);
        assert_eq!(LsbCarriesBig::decode_exact(&wire).unwrap(), m);
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
