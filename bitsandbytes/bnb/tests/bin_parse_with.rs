//! `parse_with`/`write_with` (ROADMAP Phase 2): the field-level custom-codec escape
//! hatch. `#[br(parse_with = f)]` reads the field with `f(r) -> Result<T, BitError>`
//! and `#[bw(write_with = f)]` writes it with `f(&self.field, w) -> Result<(), _>`.

mod macro_ {

    use bnb::{BitError, Sink, Source, bin, u4};

    // A length-prefixed byte run: a u8 count, then that many bytes (read at whatever
    // bit offset the cursor is at — the point of the escape hatch).
    fn parse_lp<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
        let n: u8 = r.read()?;
        let mut v = Vec::new();
        for _ in 0..n {
            v.push(r.read::<u8>()?);
        }
        Ok(v)
    }

    fn write_lp<K: Sink>(v: &[u8], w: &mut K) -> Result<(), BitError> {
        w.write(v.len() as u8)?;
        for b in v {
            w.write(*b)?;
        }
        Ok(())
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        tag: u4,
        #[br(parse_with = parse_lp)]
        #[bw(write_with = write_lp)]
        data: Vec<u8>,
    }

    #[test]
    fn custom_codec_round_trips() {
        let f = Frame {
            tag: u4::new(0x5),
            data: vec![0xAA, 0xBB, 0xCC],
        };
        let bytes = f.to_bytes().unwrap();
        assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    }

    #[test]
    fn empty_custom_run() {
        let f = Frame {
            tag: u4::new(0),
            data: vec![],
        };
        assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }

    // ——— the shipped `bnb::codecs` library, end-to-end through `#[bin]` ———

    /// Two leb128 fields of *different* widths — the field type pins the codec's width
    /// by inference (one `parse` path serves both).
    #[bin]
    #[derive(Debug, PartialEq)]
    struct Varints {
        #[br(parse_with = bnb::codecs::leb128::parse)]
        #[bw(write_with = bnb::codecs::leb128::write)]
        length: u32,
        #[br(parse_with = bnb::codecs::leb128::parse)]
        #[bw(write_with = bnb::codecs::leb128::write)]
        timestamp: u64,
    }

    #[test]
    fn shipped_leb128_width_inference() {
        let v = Varints {
            length: 300,
            timestamp: u64::MAX,
        };
        let bytes = v.to_bytes().unwrap();
        assert_eq!(&bytes[..2], &[0xAC, 0x02]); // 300, minimal form
        assert_eq!(Varints::decode_exact(&bytes).unwrap(), v);
    }

    #[test]
    fn shipped_leb128_adversarial_names_the_field() {
        // length = 5-byte overflow for u32 → the error carries the *field* name.
        let wire = [0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x00];
        let err = Varints::decode_exact(&wire).unwrap_err();
        assert_eq!(err.field, Some("length"));
    }

    /// Raw-bytes and UTF-8 cstring forms in one struct — proves the struct write path's
    /// deref coercion (`&Vec<u8>` → `&[u8]`, `&String` → `&str`).
    #[bin]
    #[derive(Debug, PartialEq)]
    struct CStrings {
        #[br(parse_with = bnb::codecs::cstring::parse)]
        #[bw(write_with = bnb::codecs::cstring::write)]
        #[try_str]
        raw: Vec<u8>,
        #[br(parse_with = bnb::codecs::cstring::parse_utf8)]
        #[bw(write_with = bnb::codecs::cstring::write_utf8)]
        title: String,
    }

    #[test]
    fn shipped_cstring_both_forms() {
        let c = CStrings {
            raw: b"abc".to_vec(),
            title: "héllo".into(),
        };
        let bytes = c.to_bytes().unwrap();
        assert_eq!(CStrings::decode_exact(&bytes).unwrap(), c);
    }

    /// The turbofish attribute form — a `u16` and a `uN` (`u12`) prefix.
    #[bin]
    #[derive(Debug, PartialEq)]
    struct Prefixed {
        #[br(parse_with = bnb::codecs::prefixed::parse_string::<_, u16>)]
        #[bw(write_with = bnb::codecs::prefixed::write_string::<_, u16>)]
        wide: String,
        #[br(parse_with = bnb::codecs::prefixed::parse_string::<_, bnb::u12>)]
        #[bw(write_with = bnb::codecs::prefixed::write_string::<_, bnb::u12>)]
        packed: String,
    }

    #[test]
    fn shipped_prefixed_turbofish_and_un() {
        let p = Prefixed {
            wide: "hello".into(),
            packed: "hi".into(),
        };
        let bytes = p.to_bytes().unwrap();
        // u16(5) + "hello" = 7 bytes, then a 12-bit length + "hi" straddling.
        assert_eq!(&bytes[..7], &[0x00, 0x05, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(Prefixed::decode_exact(&bytes).unwrap(), p);
    }

    /// The shipped codecs in a tagged-union *variant* — proves the match-bound-reference
    /// write path coerces the same way as the struct path.
    #[bin]
    #[derive(Debug, PartialEq)]
    enum Item {
        #[bin(magic = 0x01u8)]
        Named {
            #[br(parse_with = bnb::codecs::cstring::parse_utf8)]
            #[bw(write_with = bnb::codecs::cstring::write_utf8)]
            name: String,
        },
        #[bin(magic = 0x02u8)]
        Sized {
            #[br(parse_with = bnb::codecs::leb128::parse)]
            #[bw(write_with = bnb::codecs::leb128::write)]
            size: u64,
        },
    }

    #[test]
    fn shipped_codecs_in_enum_variants() {
        let n = Item::Named { name: "x".into() };
        let bytes = n.to_bytes().unwrap();
        assert_eq!(bytes, [0x01, b'x', 0x00]);
        assert_eq!(Item::decode_exact(&bytes).unwrap(), n);

        let s = Item::Sized { size: 300 };
        let bytes = s.to_bytes().unwrap();
        assert_eq!(bytes, [0x02, 0xAC, 0x02]);
        assert_eq!(Item::decode_exact(&bytes).unwrap(), s);
    }
}
