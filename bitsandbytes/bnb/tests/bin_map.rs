//! `map`/`try_map` (ROADMAP Phase 2, P2.6): transform between the wire repr and the
//! field type. `map` is infallible; `try_map` is fallible — a conversion error
//! becomes `ErrorKind::Convert`. The matched inverse is `#[bw(map = …)]`.

mod macro_ {

    use bnb::{ErrorKind, bin, u4};

    // Field type differs from its wire repr (a newtype over a different int).
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Celsius(i16);

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Reading {
        tag: u4,
        #[br(map = |raw: u16| Celsius(raw as i16))]
        #[bw(map = |c: &Celsius| c.0 as u16)]
        temp: Celsius, // wire repr is u16 (from the map's argument), straddling bytes
    }

    #[test]
    fn map_transforms_wire_and_back() {
        for t in [0i16, -5, 1000, i16::MIN, i16::MAX] {
            let r = Reading {
                tag: u4::new(0xA),
                temp: Celsius(t),
            };
            assert_eq!(Reading::decode_exact(&r.to_bytes().unwrap()).unwrap(), r);
        }
    }

    // A fallible conversion (a fn path, as in the design's `SyncPattern::try_from`).
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Small(u8);

    fn to_small(raw: u8) -> Result<Small, String> {
        if raw <= 100 {
            Ok(Small(raw))
        } else {
            Err(format!("{raw} exceeds 100"))
        }
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Msg {
        flag: u8,
        #[br(try_map = to_small)]
        #[bw(map = |s: &Small| s.0)]
        value: Small,
    }

    #[test]
    fn try_map_ok_round_trips() {
        let m = Msg {
            flag: 1,
            value: Small(42),
        };
        assert_eq!(Msg::decode_exact(&m.to_bytes().unwrap()).unwrap(), m);
    }

    #[test]
    fn try_map_failure_is_a_convert_error() {
        // flag = 0x00, value wire byte = 200 (> 100) -> the converter errors.
        let err = Msg::decode_exact(&[0x00, 200]).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Convert { .. }));
        assert_eq!(err.field, Some("value"));
    }
}
