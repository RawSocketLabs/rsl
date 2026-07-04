//! Edge cases and gaps not covered elsewhere: the `Error` → `BitError` bridge,
//! `restore_position` over a non-slice seekable source, full-width (128-bit) fields
//! through the codec, and deep nesting.

mod macro_ {

    use bnb::{BitError, BufSource, Error, ErrorKind, Source, bin, u4, u127};

    // --- the `From<Error> for BitError` bridge ------------------------------------

    #[test]
    fn construction_error_bridges_into_bit_error() {
        let e = Error::ValueTooLarge { value: 20, bits: 4 };
        let be: BitError = e.into();
        assert!(matches!(be.kind, ErrorKind::Convert { .. }));
        assert_eq!(be.at, 0); // no cursor context for a borrowed construction failure
    }

    /// A custom `parse_with` that builds a `u4` with `try_new` and lets the construction
    /// error `?`-propagate as a `BitError` (the bridge in action).
    fn read_nibble<S: Source>(r: &mut S) -> Result<u4, BitError> {
        let raw: u8 = r.read()?;
        Ok(u4::try_new(raw)?) // bnb::Error -> BitError via `?`
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct Small {
        #[br(parse_with = read_nibble)]
        v: u4,
    }

    #[test]
    fn parse_with_propagates_construction_error() {
        assert_eq!(Small::decode_exact(&[0x05]).unwrap().v, u4::new(5)); // in range
        let err = Small::decode_exact(&[0xFF]).unwrap_err(); // 0xFF doesn't fit in 4 bits
        assert!(matches!(err.kind, ErrorKind::Convert { .. }));
    }

    // --- restore_position over a non-slice seekable source ------------------------

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Peeked {
        #[br(restore_position)]
        tag: u8,
        full: u16,
    }

    #[test]
    fn restore_position_works_over_buf_source() {
        // `BufSource` is a `SeekSource`; the rewind must work at runtime (not just over a
        // slice `BitReader`), seeking back within the retained buffer.
        let mut src = BufSource::new(&[0xAB, 0xCD][..]);
        let p = Peeked::decode(&mut src).unwrap();
        assert_eq!(p.tag, 0xAB); // peeked
        assert_eq!(p.full, 0xABCD); // then re-read as the high byte of the u16
    }

    // --- full-width fields through the codec --------------------------------------

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Wide128 {
        a: u128,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Wide127 {
        a: u127,
        tail: bool, // 127 + 1 = 128 bits = 16 bytes
    }

    #[test]
    fn full_width_fields_round_trip() {
        let w = Wide128 {
            a: 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210,
        };
        let bytes = w.to_bytes().unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(Wide128::decode_exact(&bytes).unwrap(), w);

        let w = Wide127 {
            a: u127::from_raw(u128::MAX),
            tail: true,
        };
        let bytes = w.to_bytes().unwrap();
        assert_eq!(bytes.len(), 16);
        let back = Wide127::decode_exact(&bytes).unwrap();
        assert_eq!(back.a, u127::from_raw(u128::MAX));
        assert!(back.tail);
    }

    // --- deep nesting -------------------------------------------------------------

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Inner {
        a: u4,
        b: u4,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Middle {
        #[nested]
        inner: Inner,
        c: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Outer {
        #[nested]
        middle: Middle,
        d: u8,
    }

    #[test]
    fn three_levels_of_nesting_round_trip() {
        let o = Outer {
            middle: Middle {
                inner: Inner {
                    a: u4::new(1),
                    b: u4::new(2),
                },
                c: 3,
            },
            d: 4,
        };
        let bytes = o.to_bytes().unwrap();
        assert_eq!(bytes, [0x12, 0x03, 0x04]); // (1<<4 | 2), c, d
        assert_eq!(Outer::decode_exact(&bytes).unwrap(), o);
    }
}
