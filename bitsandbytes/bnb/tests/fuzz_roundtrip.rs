//! Property-based robustness + round-trip tests across a spread of `#[bin]` message
//! shapes. Three properties, each over random input:
//!
//! 1. **encode ∘ decode = id** — a value encoded then decoded is unchanged.
//! 2. **decode never panics on arbitrary bytes** — the dual-use safety property: a
//!    parser fed hostile/garbage input returns `Ok` or `Err`, never panics.
//! 3. **decode ∘ encode = id** — for fixed-length total parsers, decoding any byte
//!    string of the right length then re-encoding reproduces it (a bijection).

mod property {

    use bnb::{BitEnum, bin, u4, u12};
    use proptest::prelude::*;

    // --- shapes -------------------------------------------------------------------

    /// Byte-aligned header — a total parser, 8 bytes.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Header {
        a: u16,
        b: u16,
        c: u32,
    }

    /// Sub-byte frame — total, 16 bits = 2 bytes.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Frame {
        tag: u4,
        len: u12,
    }

    /// A catch-all enum makes decode total over its width.
    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    #[bit_enum(u8)]
    #[repr(u8)]
    enum Kind {
        A = 1,
        B = 2,
        #[catch_all]
        Other(u8),
    }

    /// Enum + scalar — total, 3 bytes.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Tagged {
        kind: Kind,
        value: u16,
    }

    /// A count-driven `Vec` whose length is derived (never stored).
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Counted {
        #[br(temp)]
        #[bw(calc = self.items.len() as u8)]
        n: u8,
        #[br(count = n)]
        items: Vec<u16>,
    }

    /// The same wire shape via the `count_prefix` sugar — must be a pure desugar of
    /// `Counted` (byte-identical output on every input).
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct CountedPrefixed {
        #[brw(count_prefix = u8)]
        items: Vec<u16>,
    }

    /// A conditional `Option`.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Optional {
        present: u8,
        #[br(if(present != 0))]
        ext: Option<u16>,
    }

    /// A magic-prefixed message — decode of arbitrary bytes usually fails `BadMagic`.
    #[bin(big, magic = 0xABCDu16)]
    #[derive(Debug, Clone, PartialEq)]
    struct Magic {
        body: u16,
    }

    /// A codec newtype — the per-type dual of the per-field attrs in `Coded` below.
    #[bin(codec = bnb::codecs::leb128)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct VarU64(u64);

    /// The flagship embedding: a variable-length newtype in an otherwise-fixed parent.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct VarFrame {
        kind: u8,
        #[brw(variable)]
        length: VarU64,
        crc: u16,
    }

    /// The shipped `bnb::codecs` library through `#[bin]` — leb128 (two widths),
    /// a length-prefixed String, and a NUL-terminated byte run.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct Coded {
        #[br(parse_with = bnb::codecs::leb128::parse)]
        #[bw(write_with = bnb::codecs::leb128::write)]
        small: u32,
        #[br(parse_with = bnb::codecs::leb128::parse)]
        #[bw(write_with = bnb::codecs::leb128::write)]
        big: u64,
        #[br(parse_with = bnb::codecs::prefixed::parse_string::<_, u16>)]
        #[bw(write_with = bnb::codecs::prefixed::write_string::<_, u16>)]
        title: String,
        #[br(parse_with = bnb::codecs::cstring::parse)]
        #[bw(write_with = bnb::codecs::cstring::write)]
        tail: Vec<u8>,
    }

    proptest! {
        // --- 1. encode ∘ decode = id over random values --------------------------

        #[test]
        fn header_roundtrips(a in any::<u16>(), b in any::<u16>(), c in any::<u32>()) {
            let h = Header { a, b, c };
            let bytes = h.to_bytes().unwrap();
            prop_assert_eq!(bytes.len(), 8);
            prop_assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
        }

        #[test]
        fn frame_roundtrips(tag in any::<u8>(), len in any::<u16>()) {
            let f = Frame { tag: u4::from_raw(tag), len: u12::from_raw(len) };
            prop_assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
        }

        #[test]
        fn tagged_roundtrips(kind in any::<u8>(), value in any::<u16>()) {
            let t = Tagged { kind: Kind::from(kind), value };
            prop_assert_eq!(Tagged::decode_exact(&t.to_bytes().unwrap()).unwrap(), t);
        }

        #[test]
        fn counted_roundtrips(items in prop::collection::vec(any::<u16>(), 0..50)) {
            let c = Counted { items: items.clone() };
            let decoded = Counted::decode_exact(&c.to_bytes().unwrap()).unwrap();
            prop_assert_eq!(decoded.items, items);
        }

        #[test]
        fn codec_newtype_roundtrips(v in any::<u64>()) {
            let bytes = VarU64(v).to_bytes().unwrap();
            prop_assert_eq!(VarU64::decode_exact(&bytes).unwrap(), VarU64(v));
        }

        #[test]
        fn codec_newtype_matches_per_field_attrs(v in any::<u64>()) {
            // The newtype's wire bytes are identical to the same value written through
            // the per-field parse_with/write_with attrs (`Coded.big`) — pure reuse,
            // not a different encoding.
            let newtype_bytes = VarU64(v).to_bytes().unwrap();
            let field = Coded {
                small: 0, big: v, title: String::new(), tail: Vec::new()
            };
            let field_bytes = field.to_bytes().unwrap();
            // Coded layout: small varint (1 byte for 0) | big varint | u16 title len (2) | NUL (1)
            prop_assert_eq!(&field_bytes[1..1 + newtype_bytes.len()], &newtype_bytes[..]);
        }

        #[test]
        fn variable_frame_roundtrips(kind in any::<u8>(), v in any::<u64>(), crc in any::<u16>()) {
            let f = VarFrame { kind, length: VarU64(v), crc };
            let bytes = f.to_bytes().unwrap();
            prop_assert_eq!(VarFrame::decode_exact(&bytes).unwrap(), f);
        }

        #[test]
        fn shipped_codecs_roundtrip(
            small in any::<u32>(),
            big in any::<u64>(),
            title in "\\PC{0,120}",                                    // any printable chars
            tail in prop::collection::vec(1u8..=255, 0..40),           // NUL-free bytes
        ) {
            let c = Coded { small, big, title: title.clone(), tail };
            let bytes = c.to_bytes().unwrap();
            prop_assert_eq!(Coded::decode_exact(&bytes).unwrap(), c);
        }

        #[test]
        fn count_prefix_is_a_pure_desugar(items in prop::collection::vec(any::<u16>(), 0..50)) {
            // The sugar and the manual triad emit byte-identical wire images…
            let manual = Counted { items: items.clone() }.to_bytes().unwrap();
            let sugar = CountedPrefixed { items: items.clone() }.to_bytes().unwrap();
            prop_assert_eq!(&sugar, &manual);
            // …and the sugar round-trips.
            let decoded = CountedPrefixed::decode_exact(&sugar).unwrap();
            prop_assert_eq!(decoded.items, items);
        }

        #[test]
        fn optional_roundtrips(present in any::<u8>(), v in any::<u16>()) {
            let ext = if present != 0 { Some(v) } else { None };
            let o = Optional { present, ext };
            prop_assert_eq!(Optional::decode_exact(&o.to_bytes().unwrap()).unwrap(), o);
        }

        #[test]
        fn magic_roundtrips(body in any::<u16>()) {
            let m = Magic { body };
            let bytes = m.to_bytes().unwrap();
            prop_assert_eq!(&bytes[..2], &[0xAB, 0xCD]); // the magic prefix
            prop_assert_eq!(Magic::decode_exact(&bytes).unwrap(), m);
        }

        // --- 2. decode of arbitrary bytes never panics ---------------------------
        // (proptest fails the test if any of these panics.)

        #[test]
        fn decode_arbitrary_bytes_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
            let _ = Header::decode_exact(&bytes);
            let _ = Frame::decode_exact(&bytes);
            let _ = Tagged::decode_exact(&bytes);
            let _ = Counted::decode_exact(&bytes);
            let _ = CountedPrefixed::decode_exact(&bytes);
            let _ = Optional::decode_exact(&bytes);
            let _ = Magic::decode_exact(&bytes);
            // The other entry points must be equally robust.
            let _ = Header::peek(&bytes);
            let _ = Counted::peek(&bytes);
            let _ = Counted::decode_all(&bytes);
            let _ = Counted::decode_iter(&bytes).count();
            let _ = CountedPrefixed::peek(&bytes);
            let _ = CountedPrefixed::decode_all(&bytes);
            let _ = CountedPrefixed::decode_iter(&bytes).count();
            let _ = Coded::decode_exact(&bytes);
            let _ = Coded::peek(&bytes);
        }

        // --- 3. decode ∘ encode = id for fixed total parsers ---------------------
        // Any byte string of the right length decodes (the parser is total) and
        // re-encodes to exactly the same bytes (a bijection).

        #[test]
        fn header_is_a_bijection(bytes in any::<[u8; 8]>()) {
            let h = Header::decode_exact(&bytes).unwrap();
            prop_assert_eq!(h.to_bytes().unwrap(), bytes.to_vec());
        }

        #[test]
        fn frame_is_a_bijection(bytes in any::<[u8; 2]>()) {
            let f = Frame::decode_exact(&bytes).unwrap();
            prop_assert_eq!(f.to_bytes().unwrap(), bytes.to_vec());
        }

        #[test]
        fn tagged_is_a_bijection(bytes in any::<[u8; 3]>()) {
            let t = Tagged::decode_exact(&bytes).unwrap();
            prop_assert_eq!(t.to_bytes().unwrap(), bytes.to_vec());
        }
    }
}
