//! Struct-level wire mapping: `#[bin(map/try_map = …, bw_map = …)]` makes a *logical* type
//! serialize via a separate *wire* type. The struct's own fields are logical data; the wire
//! type owns the bytes. Decode reads the wire type then maps to logical; encode maps logical
//! to wire then writes. The generated `BitDecode`/`BitEncode` carry the mapping, so the type
//! nests and uses the whole slice surface like any other `#[bin]` message.

mod macro_ {
    use bnb::{ErrorKind, bin};

    // The wire form — a normal #[bin] message (coords stored biased by 128).
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq)]
    struct WirePoint {
        x_biased: u8,
        y_biased: u8,
    }

    // The logical form — bidirectional mapping to/from WirePoint.
    #[bin(
        map = |w: WirePoint| Point { x: w.x_biased as i16 - 128, y: w.y_biased as i16 - 128 },
        bw_map = |p: &Point| WirePoint { x_biased: (p.x + 128) as u8, y_biased: (p.y + 128) as u8 }
    )]
    #[derive(Debug, Clone, PartialEq)]
    struct Point {
        x: i16,
        y: i16,
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
    fn a_mapped_type_nests_as_a_fixed_field() {
        // It forwards FixedBitLen from the wire type, so it sizes a region in another #[bin].
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

    // A fallible mapping with try_map (the wire type here is a bare `u8`).
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
}
