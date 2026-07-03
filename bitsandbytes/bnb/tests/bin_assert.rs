//! `#[br(assert(...))]` — the **decode-time guard** (binrw-parity spelling): after a
//! field is read (and mapped), the expression must hold or decode fails with
//! `ErrorKind::Convert`. The explicit opt-in strictness escape hatch — same rejection
//! family as `magic`, closed enums, and `try_map` (values unrepresentable in the
//! domain); the default parser stays permissive. Read-only: no `bw` inverse needed.

mod macro_ {
    use bnb::bitstream::ErrorKind;
    use bnb::{BitDecode, BitEncode, bin, u4};

    /// The pure-guard shape that used to require `try_map` + an identity `bw(map)`.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Event {
        #[br(assert((1..=2).contains(&version)))]
        version: u8,
        payload: u16,
    }

    #[test]
    fn passing_assert_decodes() {
        let e = Event {
            version: 2,
            payload: 0xBEEF,
        };
        let bytes = e.to_bytes().unwrap();
        assert_eq!(Event::decode_exact(&bytes).unwrap(), e);
    }

    #[test]
    fn failing_assert_rejects_with_field_and_expr() {
        let err = Event::decode_exact(&[0x09, 0xBE, 0xEF]).unwrap_err();
        assert_eq!(err.field, Some("version"));
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message }
                if message.contains("assertion failed") && message.contains("contains")),
            "got {err:?}"
        );
    }

    #[test]
    fn encode_is_untouched_by_the_guard() {
        // The guard is read-only: an out-of-range value still ENCODES (dual-use — you
        // can forge what you would not accept).
        let forged = Event {
            version: 9,
            payload: 0,
        };
        assert_eq!(forged.to_bytes().unwrap(), [0x09, 0x00, 0x00]);
    }

    /// The custom-message form, referencing the field in the format args.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Msg {
        #[br(assert(version <= 2, "unsupported version {}", version))]
        version: u8,
    }

    #[test]
    fn custom_message_form() {
        let err = Msg::decode_exact(&[0x09]).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message == "unsupported version 9"),
            "got {err:?}"
        );
    }

    /// An assert on a later field may reference EARLIER fields (they're locals).
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Ordered {
        min: u8,
        #[br(assert(value >= min, "value {} below the declared minimum {}", value, min))]
        value: u8,
    }

    #[test]
    fn assert_sees_earlier_fields() {
        assert!(Ordered::decode_exact(&[5, 7]).is_ok());
        let err = Ordered::decode_exact(&[5, 3]).unwrap_err();
        assert_eq!(err.field, Some("value"));
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message == "value 3 below the declared minimum 5")
        );
    }

    /// Multiple asserts run in declaration order — the first failure wins.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Multi {
        #[br(assert(n >= 1, "first"))]
        #[br(assert(n <= 4, "second"))]
        n: u8,
    }

    #[test]
    fn multiple_asserts_in_order() {
        assert!(Multi::decode_exact(&[3]).is_ok());
        let err = Multi::decode_exact(&[0]).unwrap_err();
        assert!(matches!(&err.kind, ErrorKind::Convert { message } if message == "first"));
        let err = Multi::decode_exact(&[9]).unwrap_err();
        assert!(matches!(&err.kind, ErrorKind::Convert { message } if message == "second"));
    }

    /// The guard runs AFTER `map` — it sees the mapped (domain) value.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Mapped {
        #[br(map = |raw: u8| i16::from(raw) - 40)]
        #[bw(map = |c: &i16| (*c + 40) as u8)]
        #[br(assert(celsius > -40, "sensor floor breached: {}", celsius))]
        celsius: i16,
    }

    #[test]
    fn assert_sees_the_mapped_value() {
        // raw 0 maps to -40 → the guard (over the mapped value) fires.
        let err = Mapped::decode_exact(&[0x00]).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message == "sensor floor breached: -40")
        );
        assert_eq!(Mapped::decode_exact(&[50]).unwrap(), Mapped { celsius: 10 });
    }

    /// A guard on a `temp` field — the local exists even though the field isn't stored.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Framed {
        #[br(temp)]
        #[br(assert(len <= 4, "frame too long: {}", len))]
        #[bw(calc = self.data.len() as u8)]
        len: u8,
        #[br(count = len)]
        data: Vec<u8>,
    }

    #[test]
    fn assert_on_a_temp_field() {
        assert!(Framed::decode_exact(&[2, 0xAA, 0xBB]).is_ok());
        let err = Framed::decode_exact(&[9, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err();
        assert_eq!(err.field, Some("len"));
    }

    /// In a tagged-union enum variant field.
    #[bin]
    #[derive(Debug, PartialEq)]
    enum Cmd {
        #[bin(magic = 0x01u8)]
        Set {
            #[br(assert(level <= 10))]
            level: u8,
        },
        #[bin(magic = 0x02u8)]
        Get { key: u8 },
    }

    #[test]
    fn assert_in_enum_variant() {
        assert!(Cmd::decode_exact(&[0x01, 7]).is_ok());
        let err = Cmd::decode_exact(&[0x01, 99]).unwrap_err();
        assert_eq!(err.field, Some("level"));
    }

    /// Through the bare derives (shared read path — no `#[bin]` needed).
    #[derive(BitDecode, BitEncode, Debug, PartialEq)]
    struct Raw {
        tag: u4,
        #[br(assert(body.value() != 0))]
        body: u4,
    }

    #[test]
    fn assert_through_bare_derives() {
        use bnb::{BitReader, BitWriter};
        let mut w = BitWriter::new();
        w.write(u4::new(0xA)).unwrap();
        w.write(u4::new(0x0)).unwrap();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        let err = Raw::bit_decode(&mut r).unwrap_err();
        assert_eq!(err.field, Some("body"));
    }
}
