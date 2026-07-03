//! `#[bin(codec = …)]` — **codec newtypes**: a single-field tuple struct whose wire
//! form is owned by a `parse`/`write` fn pair, making a field codec reusable *per type*
//! (annotate once, use as a plain field everywhere — the dual of repeating
//! `parse_with`/`write_with` on every field). Paired with `#[brw(variable)]`, which
//! lets a variable-length newtype sit in an otherwise-fixed parent by suppressing the
//! parent's `FixedBitLen`.

mod macro_ {
    use bnb::bin;
    use bnb::bitstream::ErrorKind;

    // ——— the module-shorthand form: `codec = <module>` → module::parse / module::write ———

    /// A LEB128-encoded u64 — the codec travels with the type.
    #[bin(codec = bnb::codecs::leb128)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Varint(pub u64);

    #[test]
    fn leb128_shorthand_golden_roundtrip() {
        assert_eq!(Varint(300).to_bytes().unwrap(), [0xAC, 0x02]);
        for v in [0u64, 1, 127, 128, 300, u64::MAX] {
            let bytes = Varint(v).to_bytes().unwrap();
            assert_eq!(Varint::decode_exact(&bytes).unwrap(), Varint(v));
        }
    }

    #[test]
    fn decode_all_walks_repeated_newtypes() {
        let vals = Varint::decode_all(&[0xAC, 0x02, 0x01]).unwrap();
        assert_eq!(vals, [Varint(300), Varint(1)]);
    }

    #[test]
    fn from_conversions_both_ways() {
        assert_eq!(u64::from(Varint(7)), 7);
        assert_eq!(Varint::from(7u64), Varint(7));
    }

    // ——— the general form: `codec(parse = <f>, write = <f>)` — any fn names ———

    /// A NUL-terminated UTF-8 string via the non-default fn names.
    #[bin(codec(parse = bnb::codecs::cstring::parse_utf8, write = bnb::codecs::cstring::write_utf8))]
    #[derive(Debug, Clone, PartialEq)]
    pub struct CName(pub String);

    #[test]
    fn paren_form_cstring_string_inner() {
        let n = CName("Hi".into());
        let bytes = n.to_bytes().unwrap();
        assert_eq!(bytes, b"Hi\0");
        assert_eq!(CName::decode_exact(&bytes).unwrap(), n);
    }

    /// A u16-length-prefixed string — pins that a turbofish (comma inside `::<_, u16>`)
    /// parses as one `codec(...)` entry.
    #[bin(codec(
        parse = bnb::codecs::prefixed::parse_string::<_, u16>,
        write = bnb::codecs::prefixed::write_string::<_, u16>
    ))]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Prefixed(pub String);

    #[test]
    fn paren_form_turbofish_prefixed() {
        let p = Prefixed("Hi".into());
        let bytes = p.to_bytes().unwrap();
        assert_eq!(bytes, [0x00, 0x02, b'H', b'i']);
        assert_eq!(Prefixed::decode_exact(&bytes).unwrap(), p);
    }

    // ——— the flagship: a codec newtype as a plain field, via #[brw(variable)] ———

    /// `kind` and `crc` are fixed-width; `length` is a variable-length codec newtype —
    /// `#[brw(variable)]` keeps the parent compilable by suppressing its `FixedBitLen`.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Frame {
        kind: u8,
        #[brw(variable)]
        length: Varint,
        crc: u16,
    }

    #[test]
    fn variable_field_in_fixed_parent() {
        let f = Frame {
            kind: 1,
            length: Varint(300),
            crc: 0xBEEF,
        };
        let bytes = f.to_bytes().unwrap();
        assert_eq!(bytes, [0x01, 0xAC, 0x02, 0xBE, 0xEF]);
        assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    }

    #[test]
    fn hostile_varint_names_the_parent_field() {
        // kind, then an unterminated 11-byte continuation run: the codec's error is
        // wrapped once, by the *parent's* field read — innermost-wins.
        let mut wire = vec![0x01u8];
        wire.extend_from_slice(&[0x80; 11]);
        let err = Frame::decode_exact(&wire).unwrap_err();
        assert_eq!(err.field, Some("length"));
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("unterminated")),
            "got {err:?}"
        );
    }

    // ——— directional narrowing: the paren form may omit the unneeded fn ———

    #[bin(read_only, codec(parse = bnb::codecs::leb128::parse))]
    #[derive(Debug, PartialEq)]
    pub struct RxVarint(pub u32);

    #[bin(write_only, codec(write = bnb::codecs::leb128::write))]
    #[derive(Debug)]
    pub struct TxVarint(pub u32);

    #[test]
    fn directional_paren_forms() {
        let bytes = TxVarint(300).to_bytes().unwrap();
        assert_eq!(RxVarint::decode_exact(&bytes).unwrap(), RxVarint(300));
    }

    // ——— layout options: they back the newtype's own slice entry points ———

    #[bin(codec = bnb::codecs::leb128, little, bit_order = lsb)]
    #[derive(Debug, PartialEq)]
    pub struct LsbVarint(pub u32);

    #[test]
    fn layout_options_on_newtype() {
        use bnb::__private::BitEncode;
        let layout = <LsbVarint as BitEncode>::LAYOUT;
        assert_eq!(layout.bit, bnb::BitOrder::Lsb);
        assert_eq!(layout.byte, bnb::ByteOrder::Little);
        // LEB128 is byte-granular, so the value round-trips regardless of the order.
        let bytes = LsbVarint(300).to_bytes().unwrap();
        assert_eq!(LsbVarint::decode_exact(&bytes).unwrap(), LsbVarint(300));
    }

    // ——— composition: Vec<newtype> under the count_prefix sugar ———

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Batch {
        #[brw(count_prefix = u8)]
        items: Vec<Varint>,
    }

    #[test]
    fn vec_of_newtypes_count_prefixed() {
        let b = Batch {
            items: vec![Varint(1), Varint(300), Varint(0)],
        };
        let bytes = b.to_bytes().unwrap();
        assert_eq!(bytes, [0x03, 0x01, 0xAC, 0x02, 0x00]);
        assert_eq!(Batch::decode_exact(&bytes).unwrap(), b);
    }

    // ——— a codec newtype in a tagged-union variant field ———

    #[bin]
    #[derive(Debug, PartialEq)]
    enum Item {
        #[bin(magic = 0x01u8)]
        Sized {
            #[brw(variable)]
            size: Varint,
        },
        #[bin(magic = 0x02u8)]
        Ping { seq: u8 },
    }

    #[test]
    fn enum_variant_newtype_field() {
        let s = Item::Sized { size: Varint(300) };
        let bytes = s.to_bytes().unwrap();
        assert_eq!(bytes, [0x01, 0xAC, 0x02]);
        assert_eq!(Item::decode_exact(&bytes).unwrap(), s);
    }
}
