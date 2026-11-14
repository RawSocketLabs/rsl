//! `magic`: a leading constant verified on read and
//! emitted on write. Bit-aware — the magic may be sub-byte (`u3`), which binrw
//! cannot express, and which legitimately keeps the struct on the bit-stream codec.

mod macro_ {

    use bnb::{ErrorKind, MagicMismatch, MagicValue, bin, u3, u4, u12, u108};

    // A byte-aligned magic in front of a sub-byte message.
    #[bin(magic = 0x7Eu8)]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Framed {
        version: u4,
        payload_len: u12,
    }

    #[test]
    fn magic_round_trips() {
        let f = Framed {
            version: u4::new(4),
            payload_len: u12::new(100),
        };
        let bytes = f.to_bytes().unwrap();
        assert_eq!(bytes[0], 0x7E, "magic emitted first");
        assert_eq!(Framed::decode_exact(&bytes).unwrap(), f);
        // The magic counts toward the const length: 8 + 4 + 12 = 24 bits = 3 bytes.
        assert_eq!(<Framed as bnb::FixedBitLen>::BIT_LEN, 24);
        assert_eq!(bytes.len(), 3);
    }

    #[test]
    fn magic_mismatch_errors() {
        let mut bytes = Framed {
            version: u4::new(4),
            payload_len: u12::new(0),
        }
        .to_bytes()
        .unwrap();
        bytes[0] = 0x5A; // corrupt the magic
        let err = Framed::decode_exact(&bytes).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::BadMagic(MagicMismatch::new(0x7E, 0x5A))
        );
        assert_eq!(err.field, Some("magic"));
        assert_eq!(err.at, 0);
    }

    // A sub-byte (3-bit) magic. The data field is byte-aligned, so without the magic
    // the right-tool guard would fire — the sub-byte magic must suppress it.
    #[bin(magic = u3::new(0b110))]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct SubByteMagic {
        body: u8,
    }

    #[test]
    fn sub_byte_magic_round_trips() {
        let s = SubByteMagic { body: 0xAB };
        let bytes = s.to_bytes().unwrap();
        assert_eq!(bytes.len(), 2, "3 + 8 = 11 bits -> 2 bytes");
        assert_eq!(bytes[0] >> 5, 0b110, "the 3-bit magic leads");
        assert_eq!(SubByteMagic::decode_exact(&bytes).unwrap(), s);
    }

    // A magic wider than 64 bits (a long sync word): the error keeps what was read.
    #[bin(big, magic = u108::new(0x0755_FD7D_F75F_7D5F_7D7D_5F7F_75F7))]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct WideMagic {
        tail: u4,
    }

    #[test]
    fn a_magic_wider_than_64_bits_reports_what_was_read() {
        let ok = WideMagic { tail: u4::new(9) };
        let mut bytes = ok.to_bytes().unwrap();
        assert_eq!(WideMagic::decode_exact(&bytes).unwrap(), ok);
        bytes[0] ^= 0x80;
        let err = WideMagic::decode_exact(&bytes).unwrap_err();
        let ErrorKind::BadMagic(mismatch) = &err.kind else {
            panic!("{err:?}")
        };
        assert_eq!(mismatch.expected(), None);
        let found = 0x0755_FD7D_F75F_7D5F_7D7D_5F7F_75F7_u128 ^ (1 << 107);
        assert_eq!(mismatch.found(), Some(MagicValue::Int(found)));
        assert_eq!((err.at, err.field), (0, Some("magic")));
    }

    // A variant's own magic, checked after a prefix selected the enum: a byte string and
    // an integer, each reported as a magic that did not match, never as a text conversion.
    #[bin(big, magic = b"PK")]
    #[derive(Debug, PartialEq)]
    enum Prefixed {
        #[bin(magic = 1u8)]
        One(u8),
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Numbered {
        #[bin(magic = 0x0102u16)]
        Known(u8),
    }

    #[test]
    fn a_prefix_or_variant_magic_that_does_not_match_is_a_bad_magic() {
        let err = Prefixed::decode_exact(b"PX\x01\x07").unwrap_err();
        assert_eq!(err.kind, ErrorKind::BadMagic(MagicMismatch::bytes(b"PK")));
        assert_eq!((err.at, err.field), (0, Some("magic")));
        assert_eq!(
            err.to_string(),
            "bad magic: expected 0x504b at bit 0 (field `magic`)"
        );
        // Decoding as a named variant checks that variant's magic.
        let err = Numbered::decode_as_known(&[0x01, 0x03, 0x07]).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::BadMagic(MagicMismatch::new(0x0102, 0x0103))
        );
        assert_eq!((err.at, err.field), (0, Some("magic")));
        // The variant's magic after a prefix that matched: reported at the bit it starts.
        let err = Prefixed::decode_as_one(b"PK\x02\x07").unwrap_err();
        assert_eq!(err.kind, ErrorKind::BadMagic(MagicMismatch::new(1, 2)));
        assert_eq!((err.at, err.field), (16, Some("magic")));
    }
}
