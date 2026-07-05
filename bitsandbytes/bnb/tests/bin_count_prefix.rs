//! `#[brw(count_prefix = <Ty>)]` — the length-prefixed count sugar (ROADMAP: findings
//! from the examples review, item 1). One directive on the `Vec` desugars to the
//! `#[br(temp)]` / `#[bw(calc = …)]` / `#[br(count = …)]` triad: the prefix is read into
//! a hidden local, sizes the `Vec`, and is recomputed from `len()` on write — derived,
//! never stored, can never drift. Encode is **checked**: an oversized collection is a
//! `BitError`, never a silently wrapped prefix.

mod macro_ {
    use bnb::bitstream::ErrorKind;
    use bnb::{bin, u4, u12};

    /// The sugar on a leaf `Vec<u8>` (sub-byte `tag` keeps the layout bit-aware).
    #[bin]
    #[derive(Debug, PartialEq)]
    struct Msg {
        tag: u4,
        #[brw(count_prefix = u16)]
        items: Vec<u8>,
    }

    /// The same wire shape, hand-written — the desugar target.
    #[bin]
    #[derive(Debug, PartialEq)]
    struct Manual {
        tag: u4,
        #[br(temp)]
        #[bw(calc = u16::try_from(self.items.len()).expect("test lengths fit"))]
        n: u16,
        #[br(count = n)]
        items: Vec<u8>,
    }

    #[test]
    fn round_trips_and_prefix_is_not_stored() {
        // A struct literal and the builder take no count argument — the prefix is invisible.
        let m = Msg {
            tag: u4::new(5),
            items: vec![0xAA, 0xBB, 0xCC],
        };
        let bytes = m.to_bytes().unwrap();
        let decoded = Msg::decode_exact(&bytes).unwrap();
        assert_eq!(decoded, m);

        let built = Msg::builder()
            .tag(u4::new(5))
            .items(vec![0xAA, 0xBB, 0xCC])
            .build()
            .unwrap();
        assert_eq!(built, m);
    }

    #[test]
    fn golden_bytes_and_byte_identity_with_the_manual_triad() {
        let sugar = Msg {
            tag: u4::new(0xF),
            items: vec![0xAA, 0xBB],
        };
        let manual = Manual {
            tag: u4::new(0xF),
            items: vec![0xAA, 0xBB],
        };
        let sugar_bytes = sugar.to_bytes().unwrap();
        assert_eq!(sugar_bytes, manual.to_bytes().unwrap());
        // tag(4) | count u16(16) | 2 elements — MSB packing: F, then 0x0002 straddling.
        assert_eq!(sugar_bytes, [0xF0, 0x00, 0x2A, 0xAB, 0xB0]);
    }

    #[test]
    fn zero_count_reads_empty() {
        let m = Msg {
            tag: u4::new(1),
            items: vec![],
        };
        let bytes = m.to_bytes().unwrap();
        let decoded = Msg::decode_exact(&bytes).unwrap();
        assert!(decoded.items.is_empty());
    }

    /// An arbitrary-width prefix occupies its declared bits, not the backing width.
    #[bin]
    #[derive(Debug, PartialEq)]
    struct Packed {
        tag: u4,
        #[brw(count_prefix = u12)]
        items: Vec<u8>,
    }

    #[test]
    fn uint_prefix_is_bit_native() {
        let m = Packed {
            tag: u4::new(0xA),
            items: vec![0x11, 0x22],
        };
        let bytes = m.to_bytes().unwrap();
        // tag(4) + u12 count(12) = exactly 2 bytes before the elements: 0xA002.
        assert_eq!(bytes, [0xA0, 0x02, 0x11, 0x22]);
        assert_eq!(Packed::decode_exact(&bytes).unwrap(), m);
    }

    #[test]
    fn encode_overflow_is_checked_not_truncated() {
        #[bin]
        #[derive(Debug, PartialEq)]
        struct Tiny {
            tag: u4,
            #[brw(count_prefix = u8)]
            items: Vec<u8>,
        }
        // 300 items: `as u8` would silently wrap to 44 — the sugar must refuse instead.
        let m = Tiny {
            tag: u4::new(0),
            items: vec![0u8; 300],
        };
        let err = m.to_bytes().unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("300")),
            "expected a Convert error naming the length, got {err:?}"
        );
        assert_eq!(err.field, Some("items"), "error names the user's field");
    }

    /// The sugar in a tagged-union enum variant (the `tlv.rs` shape).
    #[bin(little)]
    #[derive(Debug, PartialEq)]
    enum Field {
        #[bin(magic = 0x01u8)]
        Name {
            #[brw(count_prefix = u8)]
            text: Vec<u8>,
        },
        #[bin(magic = 0x02u8)]
        Age { age: u8 },
    }

    #[test]
    fn enum_variant_prefix_round_trips() {
        let f = Field::Name {
            text: b"hi".to_vec(),
        };
        let bytes = f.to_bytes().unwrap();
        assert_eq!(bytes, [0x01, 0x02, b'h', b'i']);
        assert_eq!(Field::decode_exact(&bytes).unwrap(), f);
    }

    /// Nested `#[bin]` message elements compose with the sugar.
    #[bin]
    #[derive(Debug, PartialEq, Clone)]
    struct Record {
        a: u8,
        b: u8,
    }

    #[bin]
    #[derive(Debug, PartialEq)]
    struct Table {
        tag: u4,
        #[brw(count_prefix = u8)]
        #[nested]
        rows: Vec<Record>,
    }

    #[test]
    fn nested_elements_round_trip() {
        let t = Table {
            tag: u4::new(2),
            rows: vec![Record { a: 1, b: 2 }, Record { a: 3, b: 4 }],
        };
        let bytes = t.to_bytes().unwrap();
        assert_eq!(Table::decode_exact(&bytes).unwrap(), t);
    }

    /// The sugar composes with a per-element `ctx` forward (the `ctx_length.rs` shape).
    #[bin(read_only, ctx(columns: u8))]
    #[derive(Debug, PartialEq)]
    struct Row {
        #[br(count = columns)]
        values: Vec<u8>,
    }

    // `Row` decodes `columns` values per row; the ctx-driven count is *not* the sugar's
    // business — this proves a count_prefix Vec forwards ctx to its elements.
    #[bin(read_only)]
    #[derive(Debug, PartialEq)]
    struct Grid {
        columns: u8,
        #[brw(count_prefix = u8)]
        #[br(ctx { columns })]
        #[nested]
        rows: Vec<Row>,
    }

    #[test]
    fn composes_with_ctx_forwarding() {
        // columns=2, then prefix says 2 rows, each row reads `columns` bytes.
        let wire = [0x02, 0x02, 0x0A, 0x0B, 0x0C, 0x0D];
        let g = Grid::decode_exact(&wire).unwrap();
        assert_eq!(g.columns, 2);
        assert_eq!(g.rows.len(), 2);
        assert_eq!(g.rows[0].values, vec![0x0A, 0x0B]);
        assert_eq!(g.rows[1].values, vec![0x0C, 0x0D]);
    }

    /// Directional structs: decode-only and encode-only both carry the sugar.
    #[bin(read_only)]
    #[derive(Debug, PartialEq)]
    struct RxOnly {
        tag: u4,
        #[brw(count_prefix = u8)]
        items: Vec<u8>,
    }

    #[bin(write_only)]
    #[derive(Debug)]
    struct TxOnly {
        tag: u4,
        #[brw(count_prefix = u8)]
        items: Vec<u8>,
    }

    #[test]
    fn read_only_and_write_only_work() {
        let tx = TxOnly {
            tag: u4::new(3),
            items: vec![0x42],
        };
        let bytes = tx.to_bytes().unwrap();
        let rx = RxOnly::decode_exact(&bytes).unwrap();
        assert_eq!(rx.items, vec![0x42]);
    }

    // ——— Adversarial: forged wire images (the sugar itself can't express a lying
    // count — that's the point — so hostile prefixes are built by hand). ———

    #[test]
    fn forged_over_count_is_a_graceful_eof() {
        // tag(4)=0 | u16 count = 5 | only 2 element bytes follow.
        let mut w = bnb::bitstream::BitWriter::new();
        w.write(u4::new(0)).unwrap();
        w.write(5u16).unwrap();
        w.write(0xAAu8).unwrap();
        w.write(0xBBu8).unwrap();
        let wire = w.into_bytes();
        let err = Msg::decode_exact(&wire).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::UnexpectedEof { .. }),
            "got {err:?}"
        );
        assert_eq!(err.field, Some("items"));
    }

    #[test]
    fn forged_huge_count_does_not_preallocate() {
        // A u16::MAX count with no payload: the push-based loop hits EOF immediately
        // instead of pre-allocating 65535 elements.
        let mut w = bnb::bitstream::BitWriter::new();
        w.write(u4::new(0)).unwrap();
        w.write(u16::MAX).unwrap();
        let wire = w.into_bytes();
        let err = Msg::decode_exact(&wire).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    #[test]
    fn forged_under_count_is_trailing_bytes() {
        // Count says 1, but two element bytes follow — decode_exact rejects the excess.
        let mut w = bnb::bitstream::BitWriter::new();
        w.write(u4::new(0)).unwrap();
        w.write(1u16).unwrap();
        w.write(0xAAu8).unwrap();
        w.write(0xBBu8).unwrap();
        let wire = w.into_bytes();
        let err = Msg::decode_exact(&wire).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::TrailingBytes { .. }),
            "got {err:?}"
        );
    }
}
