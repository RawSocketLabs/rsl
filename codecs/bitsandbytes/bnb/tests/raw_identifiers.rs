//! A raw identifier (`r#type`) is a spelling, not a name: where generated code *prints* a
//! field or variant (a `BitError`'s field, a missing builder field, `canonical_diff`, a
//! generated `Debug`), it prints `type`, as `std`'s own derives do.

mod macro_ {
    use bnb::{BuilderError, bin, bitfield, u4};

    #[bin(big)]
    #[derive(Debug, PartialEq, Clone)]
    struct Message {
        r#type: u8,
        #[reserved]
        r#loop: u8,
    }

    #[bitfield(u8)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Packed {
        r#type: u4,
        r#fn: u4,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Named {
        #[bin(magic = 1u8)]
        r#Type {
            r#match: u8,
            #[br(count = 1)]
            #[try_str]
            r#ref: Vec<u8>,
        },
    }

    #[test]
    fn a_missing_builder_field_is_named_without_the_raw_prefix() {
        let missing = Message::builder().build().unwrap_err();
        assert_eq!(missing, BuilderError::MissingField("type"));
    }

    #[test]
    fn a_decode_error_names_its_field_without_the_raw_prefix() {
        let short = Message::decode_exact(&[]).unwrap_err();
        assert_eq!(short.field, Some("type"));
    }

    #[test]
    fn canonical_diff_names_a_field_without_the_raw_prefix() {
        let stale = Message {
            r#type: 1,
            r#loop: 9,
        };
        assert_eq!(stale.canonical_diff(), ["loop"]);
    }

    #[test]
    fn generated_debug_prints_names_as_the_std_derive_does() {
        let packed = Packed::new().with_type(u4::new(3)).with_fn(u4::new(4));
        assert_eq!(format!("{packed:?}"), "Packed { type: u4(3), fn: u4(4) }");

        // The `#[try_str]` field makes `#[bin]` write this `Debug` itself.
        let named = Named::r#Type {
            r#match: 7,
            r#ref: b"k".to_vec(),
        };
        assert_eq!(format!("{named:?}"), r#"Type { match: 7, ref: "k" }"#);
    }
}
