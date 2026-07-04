//! `#[reserved]` / `#[reserved_with]`: a reserved field is a normal **stored** field
//! with a known *spec value* (the type's zero, or the `reserved_with` expression). On
//! the verbatim path (`decode`/`to_bytes`) it reads/writes its actual value — so a peer's
//! reserved bits are observable and overridable (dual-use). The builder defaults it to the
//! spec value (so it isn't required), and the **canonical** encoder (`to_canonical_bytes`)
//! writes the spec value instead.

mod macro_ {

    use bnb::{bin, u3, u4};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        version: u4,
        #[reserved]
        rsv: u4,
        payload: u8,
    }

    #[test]
    fn builder_defaults_reserved_to_spec_but_allows_override() {
        // Optional in the builder: omitting it defaults to the spec value (0).
        let f = Frame::builder()
            .version(u4::new(5))
            .payload(0xAB)
            .build()
            .unwrap();
        assert_eq!(f.rsv, u4::new(0));
        assert_eq!(f.to_bytes().unwrap(), [0x50, 0xAB]); // version(0101) reserved(0000)

        // ...but you can override it to emit non-spec reserved bits.
        let g = Frame::builder()
            .version(u4::new(5))
            .rsv(u4::new(0xF))
            .payload(0xAB)
            .build()
            .unwrap();
        assert_eq!(g.to_bytes().unwrap(), [0x5F, 0xAB]); // verbatim: the actual value on the wire
        assert_eq!(g.to_canonical_bytes().unwrap(), [0x50, 0xAB]); // canonical: the spec value
    }

    #[test]
    fn decode_is_verbatim_canonical_encode_normalizes() {
        // The decoder is always verbatim — it captures the actual reserved bits from the wire.
        let actual = Frame::decode_exact(&[0x5F, 0xAB]).unwrap();
        assert_eq!(actual.rsv, u4::new(0xF));
        assert_eq!(actual.version, u4::new(5));
        assert_eq!(actual.payload, 0xAB);

        // `to_bytes` re-emits those bits exactly (a faithful round-trip)...
        assert_eq!(actual.to_bytes().unwrap(), [0x5F, 0xAB]);
        // ...while `to_canonical_bytes` forces the reserved field to its spec value (0).
        assert_eq!(actual.to_canonical_bytes().unwrap(), [0x50, 0xAB]);
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame2 {
        tag: u4,
        #[reserved_with(u3::new(0b111))]
        must_be_one: u3,
        rest: u4,
    }

    #[test]
    fn reserved_with_spec_value_is_the_pattern() {
        // The builder defaults the reserved field to the must-be-one pattern.
        let f = Frame2::builder()
            .tag(u4::new(0xA))
            .rest(u4::new(0x5))
            .build()
            .unwrap();
        assert_eq!(f.must_be_one, u3::new(0b111));
        assert_eq!(f.to_bytes().unwrap()[0], 0xAE); // tag(1010) reserved(111) rest-high(0)

        // Override to a non-spec value: `to_bytes` puts it on the wire verbatim, while
        // `to_canonical_bytes` forces the pattern back. (The builder lets a caller set the
        // reserved field to a non-spec value on purpose — dual-use.)
        let g = Frame2::builder()
            .tag(u4::new(0xA))
            .must_be_one(u3::new(0))
            .rest(u4::new(0x5))
            .build()
            .unwrap();
        assert_eq!(g.to_bytes().unwrap()[0], 0xA0); // verbatim: reserved(000)
        assert_eq!(g.to_canonical_bytes().unwrap()[0], 0xAE); // canonical: reserved forced to 111

        // Decode is verbatim: it reads back exactly the non-spec bits we wrote.
        assert_eq!(
            Frame2::decode_exact(&g.to_bytes().unwrap())
                .unwrap()
                .must_be_one,
            u3::new(0)
        );
    }
}
