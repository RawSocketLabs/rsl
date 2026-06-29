//! Builder composition (ROADMAP Phase 1, chunk C): a codec struct derives
//! `BitsBuilder` alongside `BitDecode`/`BitEncode` — required-by-default
//! construction that round-trips through the codec. (Folding the builder into one
//! `#[bin]` macro is Phase 2; for now it composes as a separate derive.)

mod macro_ {

    use bnb::{BitDecode, BitEncode, BitsBuilder, BuilderError, u4, u12};

    #[derive(BitDecode, BitEncode, BitsBuilder, Debug, PartialEq, Eq, Clone, Copy)]
    struct Header {
        version: u4,
        #[builder(default)] // optional -> 0 if unset
        flags: u4,
        payload_len: u12, // 4 + 4 + 12 = 20 bits
    }

    #[test]
    fn builds_with_defaults_and_round_trips() {
        let h = Header::builder()
            .version(u4::new(4))
            .payload_len(u12::new(100))
            // flags omitted -> #[builder(default)] -> 0
            .build()
            .unwrap();
        assert_eq!(
            h,
            Header {
                version: u4::new(4),
                flags: u4::new(0),
                payload_len: u12::new(100),
            }
        );

        // The built value round-trips through the codec.
        let bytes = h.to_bytes().unwrap();
        assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
    }

    #[test]
    fn required_fields_are_enforced() {
        // version + payload_len are required (no `#[builder(default)]`).
        let err = Header::builder().version(u4::new(4)).build().unwrap_err();
        assert_eq!(err, BuilderError::MissingField("payload_len"));
    }
}
