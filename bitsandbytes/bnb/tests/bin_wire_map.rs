//! Struct-level wire mapping: a *logical* `#[bin]` type serializes via a separate *wire* type.
//! Two forms — the closure form (`map`/`try_map` + `bw_map`) and the conversion-trait form
//! (`wire`/`try_wire`, driven by `From`/`TryFrom`). Neither auto-implements `FixedBitLen` (so a
//! variable-length wire form works out of the box); to nest a *fixed*-wire mapped type as a plain
//! field, add a one-line `impl FixedBitLen`.

mod macro_ {
    use bnb::{ErrorKind, bin};

    // ===================================================================================
    // Closure form — map / try_map / bw_map
    // ===================================================================================

    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct WirePoint {
        x_biased: u8,
        y_biased: u8,
    }

    #[bin(
        map = |w: WirePoint| Point { x: w.x_biased as i16 - 128, y: w.y_biased as i16 - 128 },
        bw_map = |p: &Point| WirePoint { x_biased: (p.x + 128) as u8, y_biased: (p.y + 128) as u8 }
    )]
    #[derive(Debug, Clone, PartialEq)]
    struct Point {
        x: i16,
        y: i16,
    }
    // The mapping doesn't auto-emit FixedBitLen; this one-liner lets a fixed-wire mapped type
    // nest as a plain field (and only compiles because WirePoint is fixed-length).
    impl bnb::FixedBitLen for Point {
        const BIT_LEN: u32 = <WirePoint as bnb::FixedBitLen>::BIT_LEN;
    }

    #[test]
    fn bidirectional_round_trip() {
        let p = Point { x: -10, y: 20 };
        let bytes = p.to_bytes().unwrap();
        assert_eq!(bytes, [118, 148]); // -10+128, 20+128
        assert_eq!(Point::decode_exact(&bytes).unwrap(), p);
    }

    #[test]
    fn decode_all_walks_repeated_wire_messages() {
        assert_eq!(
            Point::decode_all(&[118, 148, 128, 128]).unwrap(),
            vec![Point { x: -10, y: 20 }, Point { x: 0, y: 0 }],
        );
    }

    #[test]
    fn a_mapped_type_nests_via_a_manual_fixed_bit_len() {
        #[bin(big)]
        #[derive(Debug, PartialEq)]
        struct Frame {
            tag: u8,
            p: Point,
        }
        let f = Frame {
            tag: 7,
            p: Point { x: 1, y: 2 },
        };
        assert_eq!(f.to_bytes().unwrap(), [7, 129, 130]);
        assert_eq!(Frame::decode_exact(&[7, 129, 130]).unwrap(), f);
    }

    // A fallible closure mapping with try_map (the wire type here is a bare `u8`).
    #[bin(
        try_map = |w: u8| if w < 100 { Ok(Pct(w)) } else { Err("percent over 100") },
        bw_map = |p: &Pct| p.0
    )]
    #[derive(Debug, Clone, PartialEq)]
    struct Pct(u8);

    #[test]
    fn try_map_accepts_valid_and_round_trips() {
        assert_eq!(Pct::decode_exact(&[42]).unwrap(), Pct(42));
        assert_eq!(Pct(99).to_bytes().unwrap(), [99]);
    }

    #[test]
    fn try_map_failure_is_a_convert_error() {
        let err = Pct::decode_exact(&[200]).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Convert { .. }));
    }

    // Read-only mapped: only `map` (no bw_map) generates the decode side alone.
    #[bin(map = |w: u16| Tick(w))]
    #[derive(Debug, PartialEq)]
    struct Tick(u16);

    #[test]
    fn read_only_mapped_decodes() {
        assert_eq!(Tick::decode_exact(&[0x12, 0x34]).unwrap(), Tick(0x1234));
    }

    // Write-only mapped: only `bw_map`, with the wire type from the annotated return.
    #[bin(bw_map = |s: &Token| -> u16 { s.0 })]
    #[derive(Debug, PartialEq)]
    struct Token(u16);

    #[test]
    fn write_only_mapped_encodes() {
        assert_eq!(Token(0xABCD).to_bytes().unwrap(), [0xAB, 0xCD]);
    }

    // ===================================================================================
    // Conversion-trait form — wire / try_wire (From<Wire> / TryFrom<Wire> + From<&Self>)
    // ===================================================================================

    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct WireCoord {
        x: u8,
        y: u8,
    }

    #[bin(wire = WireCoord)] // needs From<WireCoord> for Coord + From<&Coord> for WireCoord
    #[derive(Debug, Clone, PartialEq)]
    struct Coord {
        x: i16,
        y: i16,
    }
    impl From<WireCoord> for Coord {
        fn from(w: WireCoord) -> Self {
            Coord {
                x: w.x as i16 - 128,
                y: w.y as i16 - 128,
            }
        }
    }
    impl From<&Coord> for WireCoord {
        fn from(c: &Coord) -> Self {
            WireCoord {
                x: (c.x + 128) as u8,
                y: (c.y + 128) as u8,
            }
        }
    }
    impl bnb::FixedBitLen for Coord {
        const BIT_LEN: u32 = <WireCoord as bnb::FixedBitLen>::BIT_LEN;
    }

    #[test]
    fn wire_form_round_trips_and_is_usable_in_program() {
        let c = Coord { x: -10, y: 20 };
        assert_eq!(c.to_bytes().unwrap(), [118, 148]);
        assert_eq!(Coord::decode_exact(&[118, 148]).unwrap(), c);
        // The From impls are reusable directly, not just at the codec boundary:
        let w: WireCoord = (&c).into();
        assert_eq!(w, WireCoord { x: 118, y: 148 });
        assert_eq!(
            Coord::from(WireCoord { x: 128, y: 128 }),
            Coord { x: 0, y: 0 }
        );
    }

    #[test]
    fn wire_form_nests_via_a_manual_fixed_bit_len() {
        #[bin(big)]
        #[derive(Debug, PartialEq)]
        struct Frame {
            tag: u8,
            c: Coord,
        }
        let f = Frame {
            tag: 9,
            c: Coord { x: 0, y: 1 },
        };
        assert_eq!(f.to_bytes().unwrap(), [9, 128, 129]);
        assert_eq!(Frame::decode_exact(&[9, 128, 129]).unwrap(), f);
    }

    // A VARIABLE-length wire form (count-driven Vec) — has no FixedBitLen, yet the mapped
    // logical type works as a standalone message. This is what the conversion-trait form unlocks.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct WireText {
        n: u8,
        #[br(count = n)]
        data: Vec<u8>,
    }

    #[bin(wire = WireText)]
    #[derive(Debug, Clone, PartialEq)]
    struct Text(String);
    impl From<WireText> for Text {
        fn from(w: WireText) -> Self {
            Text(String::from_utf8_lossy(&w.data).into_owned())
        }
    }
    impl From<&Text> for WireText {
        fn from(t: &Text) -> Self {
            WireText {
                n: t.0.len() as u8,
                data: t.0.as_bytes().to_vec(),
            }
        }
    }

    #[test]
    fn variable_length_wire_form_works_standalone() {
        let t = Text("hi".into());
        assert_eq!(t.to_bytes().unwrap(), [2, b'h', b'i']);
        assert_eq!(
            Text::decode_exact(&[3, b'a', b'b', b'c']).unwrap(),
            Text("abc".into())
        );
        assert_eq!(
            Text::decode_all(&[1, b'x', 2, b'y', b'z']).unwrap(),
            vec![Text("x".into()), Text("yz".into())],
        );
    }

    // Fallible conversion-trait form: try_wire uses TryFrom<Wire>.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct WirePct {
        raw: u8,
    }

    #[bin(try_wire = WirePct)] // needs TryFrom<WirePct> for Ratio + From<&Ratio> for WirePct
    #[derive(Debug, PartialEq)]
    struct Ratio(u8);
    impl TryFrom<WirePct> for Ratio {
        type Error = &'static str;
        fn try_from(w: WirePct) -> Result<Self, Self::Error> {
            if w.raw <= 100 {
                Ok(Ratio(w.raw))
            } else {
                Err("percent over 100")
            }
        }
    }
    impl From<&Ratio> for WirePct {
        fn from(r: &Ratio) -> Self {
            WirePct { raw: r.0 }
        }
    }

    #[test]
    fn try_wire_accepts_valid_and_rejects_invalid() {
        assert_eq!(Ratio::decode_exact(&[42]).unwrap(), Ratio(42));
        assert_eq!(Ratio(99).to_bytes().unwrap(), [99]);
        let err = Ratio::decode_exact(&[200]).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Convert { .. }));
    }
}
