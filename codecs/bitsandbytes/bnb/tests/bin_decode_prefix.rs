//! `Type::decode_prefix` / `decode_prefix_eof` — the stateless slice prefix decoders. Unlike
//! `peek`, a short slice is `Incomplete` (retry by replaying with more bytes), never confused
//! with malformed input; `_eof` declares the slice final. Consumed counts are slice-relative
//! whole bytes (a bit-granular message rounds up). Emitted on every context-free decode
//! surface: `#[bin]` structs, bare `#[derive(BitDecode)]`, enums, mapped types, and codec
//! newtypes.

mod macro_ {

    use bnb::{BitDecode, BitError, ErrorKind, Source, bin, u4};

    // The bare derive shares `gen_decode` with `#[bin]` structs: 4 + 4 + 4 = 12 bits.
    #[derive(BitDecode, Debug, PartialEq, Eq)]
    struct Nibbles {
        a: u4,
        b: u4,
        c: u4,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct Record {
        tag: u8,
        #[brw(count_prefix = u16)]
        body: Vec<u8>,
    }

    #[bin(big, magic = 0xCAFEu16)]
    #[derive(Debug, PartialEq, Eq)]
    struct Framed {
        v: u8,
    }

    // 12 bits: the second byte's low nibble is padding.
    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct Twelve {
        a: u4,
        b: u8,
    }

    // A one-byte region length, then a `u8`-counted run that must fit inside that region:
    // the region is a *logical* bound, so overrunning it is malformed whatever follows.
    fn parse_region<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
        let len = usize::from(r.read::<u8>()?);
        let mut region = r.limit_bytes(len)?;
        let count = region.read::<u8>()?;
        (0..count).map(|_| region.read::<u8>()).collect()
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq, Eq)]
    struct Enveloped {
        #[br(parse_with = parse_region)]
        inner: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum Address {
        #[bin(magic = 1u8)]
        V4([u8; 4]),
        #[bin(magic = 4u8)]
        V6([u8; 16]),
    }

    #[bin(try_map = |w: u8| if w < 100 { Ok(Pct(w)) } else { Err("percent over 100") })]
    #[derive(Debug, PartialEq, Eq)]
    struct Pct(u8);

    #[bin(codec = bnb::codecs::leb128)]
    #[derive(Debug, PartialEq, Eq)]
    struct Varint(u64);

    fn record_bytes() -> Vec<u8> {
        Record {
            tag: 9,
            body: vec![1, 2, 3],
        }
        .to_bytes()
        .unwrap()
    }

    #[test]
    fn complete_message_reports_bytes_consumed_and_ignores_the_tail() {
        let mut bytes = record_bytes();
        let len = bytes.len();
        bytes.extend_from_slice(&[0xee, 0xff]);
        for result in [
            Record::decode_prefix(&bytes),
            Record::decode_prefix_eof(&bytes),
        ] {
            let (record, consumed) = result.unwrap();
            assert_eq!(record.body, [1, 2, 3]);
            assert_eq!(consumed, len);
        }
    }

    #[test]
    fn every_truncation_is_incomplete_unless_declared_final() {
        let bytes = record_bytes();
        for split in 0..bytes.len() {
            let prefix = &bytes[..split];
            let short = Record::decode_prefix(prefix).unwrap_err();
            let ErrorKind::Incomplete {
                needed: Some(needed),
            } = short.kind
            else {
                panic!("split {split}: expected a positive hint, got {short}");
            };
            assert!(needed > 0 && split + needed <= bytes.len(), "split {split}");
            let final_short = Record::decode_prefix_eof(prefix).unwrap_err();
            assert!(
                matches!(final_short.kind, ErrorKind::UnexpectedEof { .. }),
                "split {split}: {final_short}"
            );
            // `peek` cannot tell the two apart: it is not a prefix decoder.
            assert_eq!(Record::peek(prefix).unwrap_err(), final_short);
        }
    }

    #[test]
    fn malformed_input_is_definitive_with_or_without_eof() {
        let bytes = [0x00, 0x00, 0x01];
        for error in [
            Framed::decode_prefix(&bytes).unwrap_err(),
            Framed::decode_prefix_eof(&bytes).unwrap_err(),
        ] {
            assert!(matches!(
                error.kind,
                ErrorKind::BadMagic {
                    expected: 0xCAFE,
                    found: 0
                }
            ));
        }
        // A partial magic that still matches is only short.
        assert!(Framed::decode_prefix(&[0xCA]).unwrap_err().is_incomplete());
        assert_eq!(
            Framed::decode_prefix(&[0xCA, 0xFE, 7, 8]).unwrap(),
            (Framed { v: 7 }, 3)
        );
    }

    #[test]
    fn region_overrun_inside_a_complete_slice_stays_definitive() {
        // Region of 2 bytes claims 5 elements; the slice holds 6 more bytes than that.
        let bytes = [2, 5, 1, 1, 1, 1, 1, 1, 1, 1];
        for error in [
            Enveloped::decode_prefix(&bytes).unwrap_err(),
            Enveloped::decode_prefix_eof(&bytes).unwrap_err(),
        ] {
            assert!(
                matches!(error.kind, ErrorKind::UnexpectedEof { .. }),
                "{error}"
            );
        }
        // A region the slice has not finished delivering is ordinary shortage.
        assert!(
            Enveloped::decode_prefix(&[3, 2, 1])
                .unwrap_err()
                .is_incomplete()
        );
        assert_eq!(
            Enveloped::decode_prefix(&[3, 2, 1, 2, 0xff]).unwrap(),
            (Enveloped { inner: vec![1, 2] }, 4)
        );
    }

    #[test]
    fn bit_granular_messages_round_consumed_bytes_up() {
        let (value, consumed) = Twelve::decode_prefix(&[0xAB, 0xCF, 0x11]).unwrap();
        assert_eq!((value.a, value.b, consumed), (u4::new(0xA), 0xBC, 2));
        assert_eq!(
            Twelve::decode_prefix(&[0xAB]).unwrap_err().kind,
            ErrorKind::Incomplete { needed: Some(1) }
        );
    }

    #[test]
    fn empty_input_is_shortage_like_any_other_truncation() {
        assert_eq!(
            Record::decode_prefix(&[]).unwrap_err().kind,
            ErrorKind::Incomplete { needed: Some(1) }
        );
        assert!(matches!(
            Record::decode_prefix_eof(&[]).unwrap_err().kind,
            ErrorKind::UnexpectedEof { remaining: 0, .. }
        ));
    }

    #[test]
    fn derives_enums_mapped_types_and_codec_newtypes_share_the_surface() {
        assert_eq!(
            Nibbles::decode_prefix(&[0x12, 0x3f, 0xff]).unwrap(),
            (
                Nibbles {
                    a: u4::new(1),
                    b: u4::new(2),
                    c: u4::new(3)
                },
                2
            )
        );
        assert!(Nibbles::decode_prefix(&[0x12]).unwrap_err().is_incomplete());
        assert!(
            !Nibbles::decode_prefix_eof(&[0x12])
                .unwrap_err()
                .is_incomplete()
        );

        assert_eq!(
            Address::decode_prefix(&[1, 10, 0, 0, 1, 0xff]).unwrap(),
            (Address::V4([10, 0, 0, 1]), 5)
        );
        assert!(Address::decode_prefix(&[4, 0]).unwrap_err().is_incomplete());
        assert!(!Address::decode_prefix(&[2]).unwrap_err().is_incomplete());

        assert_eq!(Pct::decode_prefix(&[42, 0]).unwrap(), (Pct(42), 1));
        assert!(Pct::decode_prefix(&[]).unwrap_err().is_incomplete());
        assert!(matches!(
            Pct::decode_prefix(&[200]).unwrap_err().kind,
            ErrorKind::Convert { .. }
        ));

        assert_eq!(
            Varint::decode_prefix(&[0xAC, 0x02, 0xff]).unwrap(),
            (Varint(300), 2)
        );
        assert!(Varint::decode_prefix(&[0xAC]).unwrap_err().is_incomplete());
        assert!(
            !Varint::decode_prefix_eof(&[0xAC])
                .unwrap_err()
                .is_incomplete()
        );
    }
}
