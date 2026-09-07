//! Builder alias normalization: generated surfaces and their opaque boundaries.

mod macro_ {
    use bnb::{BitEnum, Bits, BitsBuilder, BuilderError, NormalizeEnumAliases, bin, bitfield, u4};

    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    #[bit_enum(u8)]
    #[repr(u8)]
    enum Kind {
        Named = 2,
        Sentinel = 255,
        #[catch_all]
        Other(u8) = 3,
    }

    impl Default for Kind {
        fn default() -> Self {
            Self::Other(2)
        }
    }

    #[derive(BitsBuilder)]
    struct Defaulted {
        #[builder(default)]
        kind: Kind,
    }

    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    #[bit_enum(u4)]
    #[repr(u8)]
    enum Nibble {
        Named = 2,
        #[catch_all]
        Other(u4),
    }

    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    #[bit_enum(u128)]
    #[repr(u8)]
    enum Wide {
        Named = 2,
        #[catch_all]
        Other(u128),
    }

    #[test]
    fn enum_normalization_preserves_every_byte_and_is_idempotent() {
        for raw in 0..=u8::MAX {
            let mut value = Kind::Other(raw);
            value.normalize_enum_aliases();
            let expected = match raw {
                2 => Kind::Named,
                255 => Kind::Sentinel,
                _ => Kind::Other(raw),
            };
            assert_eq!(value, expected, "discriminant {raw}");
            assert_eq!(u8::from(value), raw);
            value.normalize_enum_aliases();
            assert_eq!(value, expected, "idempotence at {raw}");
            let consumed = Kind::Other(raw).into_normalized_enum_aliases();
            assert_eq!(consumed, expected, "consuming discriminant {raw}");
            assert_eq!(u8::from(consumed), raw);
            assert_eq!(consumed.into_normalized_enum_aliases(), expected);
        }
    }

    #[test]
    fn normalization_supports_subbyte_and_full_carrier_widths() {
        for raw in 0..16 {
            let mut value = Nibble::Other(u4::new(raw));
            value.normalize_enum_aliases();
            assert_eq!(value.into_bits(), u128::from(raw));
            assert_eq!(value == Nibble::Named, raw == 2);
        }
        for raw in [0, 2, u128::from(u64::MAX) + 1, u128::MAX] {
            let mut value = Wide::Other(raw);
            value.normalize_enum_aliases();
            assert_eq!(value.into_bits(), raw);
            assert_eq!(value == Wide::Named, raw == 2);
        }
    }

    #[bin(validate = require_named)]
    #[derive(Debug)]
    struct Checked {
        #[builder(default = Kind::Other(2))]
        kind: Kind,
        required: u8,
    }

    fn require_named(value: &Checked) -> Result<(), &'static str> {
        match value.kind {
            Kind::Named => Ok(()),
            _ => Err("expected named variant"),
        }
    }

    #[test]
    fn defaults_and_setters_normalize_before_validation_but_after_required_fields() {
        assert_eq!(Defaulted::builder().build().unwrap().kind, Kind::Named);
        assert_eq!(
            Checked::builder().build().unwrap_err(),
            BuilderError::MissingField("required")
        );
        assert_eq!(
            Checked::builder().required(7).build().unwrap().kind,
            Kind::Named
        );
        let mut value = Checked::builder()
            .kind(Kind::Other(2))
            .required(7)
            .build()
            .unwrap();
        assert!(value.is_valid());
        assert_eq!(value.to_bytes().unwrap(), [2, 7]);
        value.kind = Kind::Other(2);
        assert!(
            value.validate().is_err(),
            "immutable validation must not normalize mutation"
        );
        assert_eq!(value.kind, Kind::Other(2));
        assert_eq!(value.to_bytes().unwrap(), [2, 7]);
        assert_eq!(Checked::decode_exact(&[2, 7]).unwrap().kind, Kind::Named);
    }

    #[derive(Debug)]
    struct Opaque(String);

    type Aliased = Option<Vec<[Kind; 2]>>;

    #[derive(BitsBuilder)]
    struct Plain {
        values: Aliased,
        opaque: Opaque,
    }

    #[derive(BitsBuilder)]
    struct Outer {
        children: Vec<Plain>,
    }

    #[test]
    fn plain_builders_recurse_through_aliases_and_nested_containers_without_allocating() {
        let values = vec![[Kind::Other(2), Kind::Other(99)]];
        let pointer = values.as_ptr();
        let capacity = values.capacity();
        let child = Plain {
            values: Some(values),
            opaque: Opaque("retained".into()),
        };
        let outer = Outer::builder().children(vec![child]).build().unwrap();
        let values = outer.children[0].values.as_ref().unwrap();
        assert_eq!(values.as_slice(), [[Kind::Named, Kind::Other(99)]]);
        assert_eq!(values.as_ptr(), pointer);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(outer.children[0].opaque.0, "retained");
        let empty = Plain::builder()
            .values(None)
            .opaque(Opaque(String::new()))
            .build()
            .unwrap();
        assert!(empty.values.is_none());
        let mut empty_array: [Kind; 0] = [];
        empty_array.normalize_enum_aliases();
        let mut empty_vec: Vec<Kind> = Vec::new();
        empty_vec.normalize_enum_aliases();
        assert_eq!(empty_vec.capacity(), 0);
    }

    #[test]
    fn consuming_normalization_preserves_nested_container_storage_and_opaque_fields() {
        let values = vec![
            [Kind::Other(2), Kind::Other(99)],
            [Kind::Other(255), Kind::Named],
        ];
        let pointer = values.as_ptr();
        let capacity = values.capacity();
        let outer = Outer {
            children: vec![Plain {
                values: Some(values),
                opaque: Opaque("retained".into()),
            }],
        }
        .into_normalized_enum_aliases();
        let values = outer.children[0].values.as_ref().unwrap();
        assert_eq!(
            values.as_slice(),
            [
                [Kind::Named, Kind::Other(99)],
                [Kind::Sentinel, Kind::Named]
            ]
        );
        assert_eq!(values.as_ptr(), pointer);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(outer.children[0].opaque.0, "retained");
        assert_eq!(None::<Kind>.into_normalized_enum_aliases(), None);
        assert_eq!([Kind::Other(2); 0].into_normalized_enum_aliases(), []);
        let empty = Vec::<Kind>::new().into_normalized_enum_aliases();
        assert!(empty.is_empty());
        assert_eq!(empty.capacity(), 0);
    }

    #[test]
    fn consuming_normalization_does_not_validate_or_change_message_bytes() {
        let value = Checked {
            kind: Kind::Other(99),
            required: 7,
        }
        .into_normalized_enum_aliases();
        assert!(value.validate().is_err());
        assert_eq!(value.to_bytes().unwrap(), [99, 7]);
    }

    #[bitfield(u8)]
    #[derive(BitsBuilder, Clone, Copy)]
    struct Packed {
        kind: Kind,
    }

    #[test]
    fn intercepted_bitfield_builder_retains_discriminants_and_exposes_named_variants() {
        let value = Packed::builder().kind(Kind::Other(2)).build().unwrap();
        assert_eq!(value.kind(), Kind::Named);
        assert_eq!(value.to_be_bytes(), [2]);
    }

    #[bin(no_builder)]
    #[derive(Debug, PartialEq, Eq)]
    struct Nested {
        kind: Kind,
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    enum Payload {
        #[bin(magic = 1u8)]
        Tuple(Nested),
        #[bin(magic = 2u8)]
        Named {
            #[brw(count_prefix = u8)]
            kinds: Vec<Kind>,
        },
        #[bin(magic = 3u8)]
        Unit,
    }

    #[bin]
    struct Packet {
        #[brw(variable)]
        payload: Payload,
    }

    #[test]
    fn nested_bin_structs_and_all_enum_field_shapes_normalize() {
        let cases = [
            (
                Payload::Tuple(Nested {
                    kind: Kind::Other(2),
                }),
                vec![1, 2],
            ),
            (
                Payload::Named {
                    kinds: vec![Kind::Other(2), Kind::Other(99)],
                },
                vec![2, 2, 2, 99],
            ),
            (Payload::Unit, vec![3]),
        ];
        for (payload, bytes) in cases {
            let packet = Packet::builder().payload(payload).build().unwrap();
            assert_eq!(packet.to_bytes().unwrap(), bytes);
            assert_eq!(packet.payload, Payload::decode_exact(&bytes).unwrap());
        }
    }

    // These codecs deliberately observe the Rust variant, not its discriminant.
    #[allow(clippy::trivially_copy_pass_by_ref)] // bw(map) requires a borrowed field.
    fn variant_byte(kind: &Kind) -> u8 {
        match kind {
            Kind::Other(_) => 7,
            _ => 2,
        }
    }

    fn read_kind<S: bnb::Source>(source: &mut S) -> Result<Kind, bnb::BitError> {
        source.read::<u8>().map(Kind::Other)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // write_with/codec share this signature.
    fn write_kind<S: bnb::Sink>(kind: &Kind, sink: &mut S) -> Result<(), bnb::BitError> {
        sink.write(variant_byte(kind))
    }

    #[bin]
    struct Boundaries {
        ordinary: Kind,
        #[br(map = |raw: u8| Kind::Other(raw))]
        #[bw(map = variant_byte)]
        mapped: Kind,
        #[br(parse_with = read_kind)]
        #[bw(write_with = write_kind)]
        custom: Kind,
        #[brw(ignore)]
        ignored: Kind,
        #[br(calc = Kind::Other(2))]
        logical: Kind,
        #[bw(calc = Kind::Named)]
        calculated: Kind,
        #[reserved_with(Kind::Named)]
        reserved: Kind,
    }

    #[test]
    fn callback_and_logical_fields_are_opaque_and_canonical_repair_stays_separate() {
        let mut value = Boundaries::builder()
            .ordinary(Kind::Other(2))
            .mapped(Kind::Other(2))
            .custom(Kind::Other(2))
            .ignored(Kind::Other(2))
            .logical(Kind::Other(2))
            .calculated(Kind::Other(99))
            .reserved(Kind::Other(99))
            .build()
            .unwrap();
        value.ordinary = Kind::Other(2);
        let before = value.to_bytes().unwrap();
        let value = value.into_normalized_enum_aliases();
        assert_eq!(value.to_bytes().unwrap(), before);
        assert_eq!(value.ordinary, Kind::Named);
        assert_eq!(value.mapped, Kind::Other(2));
        assert_eq!(value.custom, Kind::Other(2));
        assert_eq!(value.ignored, Kind::Other(2));
        assert_eq!(value.logical, Kind::Other(2));
        assert_eq!(value.calculated, Kind::Other(99));
        assert_eq!(value.reserved, Kind::Other(99));
        assert_eq!(value.to_bytes().unwrap(), [2, 7, 7, 99, 99]);
        assert_eq!(value.to_canonical_bytes().unwrap(), [2, 7, 7, 2, 2]);
        let decoded = Boundaries::decode_exact(&[2, 7, 7, 99, 99]).unwrap();
        assert_eq!(decoded.mapped, Kind::Other(7));
        assert_eq!(decoded.custom, Kind::Other(7));
        assert_eq!(decoded.to_bytes().unwrap(), [2, 7, 7, 99, 99]);
    }

    #[derive(bnb::BitDecode, bnb::BitEncode, BitsBuilder)]
    #[bit_stream(allow_byte_aligned)]
    struct Bare {
        ordinary: Kind,
        #[br(try_map = |raw: u8| Ok::<_, &'static str>(Kind::Other(raw)))]
        #[bw(map = variant_byte)]
        mapped: Kind,
        #[br(parse_with = read_kind)]
        #[bw(write_with = write_kind)]
        custom: Kind,
        #[brw(ignore)]
        ignored: Kind,
        #[bw(calc = Kind::Named)]
        calculated: Kind,
    }

    #[test]
    fn standalone_builder_honors_bare_codec_directive_boundaries() {
        let value = Bare::builder()
            .ordinary(Kind::Other(2))
            .mapped(Kind::Other(2))
            .custom(Kind::Other(2))
            .ignored(Kind::Other(2))
            .calculated(Kind::Other(2))
            .build()
            .unwrap();
        assert_eq!(value.ordinary, Kind::Named);
        assert_eq!(value.mapped, Kind::Other(2));
        assert_eq!(value.custom, Kind::Other(2));
        assert_eq!(value.ignored, Kind::Other(2));
        assert_eq!(value.calculated, Kind::Other(2));
        assert_eq!(value.to_bytes().unwrap(), [2, 7, 7, 2]);
    }

    #[bin(wire = u8)]
    struct Mapped(Kind);

    impl From<u8> for Mapped {
        fn from(raw: u8) -> Self {
            Self(Kind::Other(raw))
        }
    }

    impl From<&Mapped> for u8 {
        fn from(value: &Mapped) -> Self {
            variant_byte(&value.0)
        }
    }

    #[bin(codec(parse = read_kind, write = write_kind))]
    struct Custom(Kind);

    #[derive(BitsBuilder)]
    struct Wrappers {
        mapped: Mapped,
        custom: Custom,
        explicit: Explicit,
    }

    struct Explicit(Kind);

    impl NormalizeEnumAliases for Explicit {
        fn normalize_enum_aliases(&mut self) {
            self.0.normalize_enum_aliases();
        }
    }

    #[test]
    fn type_level_codecs_are_opaque_unless_the_wrapper_explicitly_implements_normalization() {
        let value = Wrappers::builder()
            .mapped(Mapped(Kind::Other(2)))
            .custom(Custom(Kind::Other(2)))
            .explicit(Explicit(Kind::Other(2)))
            .build()
            .unwrap();
        assert_eq!(value.mapped.0, Kind::Other(2));
        assert_eq!(value.custom.0, Kind::Other(2));
        assert_eq!(value.explicit.0, Kind::Named);
        assert_eq!(value.mapped.to_bytes().unwrap(), [7]);
        assert_eq!(value.custom.to_bytes().unwrap(), [7]);
    }
}
