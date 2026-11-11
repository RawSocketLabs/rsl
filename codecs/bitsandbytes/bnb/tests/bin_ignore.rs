//! `#[brw(ignore)]`: a field that is neither read nor written — `Default::default()`
//! on read (no input consumed), skipped on write. Zero wire bits, but still a stored
//! (and builder) field. Spelled with `brw` because it applies to both directions.

mod macro_ {

    use bnb::{bin, u4, u12};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        tag: u4,
        payload: u12,
        #[brw(ignore)]
        note: u32, // in-memory metadata, never serialized
    }

    #[test]
    fn ignored_field_is_not_on_the_wire() {
        let f = Frame {
            tag: u4::new(0x5),
            payload: u12::new(0x123),
            note: 0xDEAD_BEEF,
        };
        let bytes = f.to_bytes().unwrap();
        assert_eq!(
            bytes.len(),
            2,
            "tag(4) + payload(12) = 16 bits; note skipped"
        );

        let decoded = Frame::decode_exact(&bytes).unwrap();
        assert_eq!(decoded.tag, f.tag);
        assert_eq!(decoded.payload, f.payload);
        assert_eq!(decoded.note, 0, "defaulted on read, not the original value");
    }

    #[test]
    fn ignored_field_is_still_a_builder_field() {
        let f = Frame::builder()
            .tag(u4::new(1))
            .payload(u12::new(2))
            .note(7)
            .build()
            .unwrap();
        assert_eq!(f.note, 7);
    }

    // A memoised field: never on the wire, but its `calc` names the value it should hold, so
    // the canonical helpers compare and recompute it.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Cached {
        raw: u8,
        #[brw(ignore)]
        #[bw(calc = u16::from(self.raw) * 2)]
        doubled: u16,
    }

    #[test]
    fn ignored_calc_field_is_defaulted_on_decode_and_recomputed_by_to_canonical() {
        let decoded = Cached::decode_exact(&[3]).unwrap();
        assert_eq!(decoded.doubled, 0, "defaulted on read: the cache is cold");
        assert_eq!(decoded.canonical_diff(), ["doubled"]);

        let warm = decoded.to_canonical();
        assert_eq!(warm, Cached { raw: 3, doubled: 6 });
        assert!(warm.is_canonical());
        assert_eq!(warm.to_bytes().unwrap(), [3], "still not on the wire");
        assert_eq!(warm.to_canonical_bytes().unwrap(), [3]);
    }
}
