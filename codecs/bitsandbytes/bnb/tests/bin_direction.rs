//! `read_only` / `write_only` narrow a `#[bin]` type to one direction, and a narrowed type
//! needs nothing from the other one: no write inverse for a read-side directive, no `count`
//! for a `Vec` that is only written, and no write-side impl on a wire type that is only
//! read.

mod macro_ {
    use bnb::bin;

    // ── A read-only enum: read-side directives with no write inverse. ──
    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    enum Incoming {
        #[bin(magic = 1u8)]
        Mapped(#[br(map = |raw: u8| u16::from(raw) * 2)] u16),
        #[bin(magic = 2u8)]
        Parsed {
            #[br(parse_with = bnb::codecs::leb128::parse)]
            size: u64,
        },
        #[bin(magic = 3u8)]
        Counted {
            #[br(temp)]
            len: u8,
            #[br(count = len)]
            body: Vec<u8>,
        },
    }

    #[test]
    fn a_read_only_enum_needs_no_write_inverses() {
        assert_eq!(
            Incoming::decode_exact(&[1, 21]).unwrap(),
            Incoming::Mapped(42)
        );
        assert_eq!(
            Incoming::decode_exact(&[2, 0x80, 0x01]).unwrap(),
            Incoming::Parsed { size: 128 }
        );
        assert_eq!(
            Incoming::decode_exact(&[3, 2, 0xAA, 0xBB]).unwrap(),
            Incoming::Counted {
                body: vec![0xAA, 0xBB]
            }
        );
        assert_eq!(
            Incoming::decode_as_mapped(&[1, 1]).unwrap(),
            Incoming::Mapped(2)
        );
    }

    // ── A write-only enum: a `Vec` with no `count`, a write-side directive with no read
    // inverse. ──
    #[bin(big, write_only)]
    enum Outgoing {
        #[bin(magic = 1u8)]
        Run { body: Vec<u8> },
        #[bin(magic = 2u8)]
        Varint(#[bw(write_with = bnb::codecs::leb128::write)] u64),
    }

    #[test]
    fn a_write_only_enum_needs_nothing_read_side() {
        let run = Outgoing::Run {
            body: vec![0xAA, 0xBB],
        };
        assert_eq!(run.to_bytes().unwrap(), [1, 0xAA, 0xBB]);
        assert_eq!(Outgoing::Varint(128).to_bytes().unwrap(), [2, 0x80, 0x01]);
    }

    // ── A read-only mapped struct over a read-only wire type (little-endian, so a wrong
    // layout would show), and the write-only dual. ──
    #[bin(little, read_only)]
    struct WireIn {
        raw: u16,
    }

    #[bin(read_only, wire = WireIn)]
    #[derive(Debug, PartialEq)]
    struct Reading(u16);

    impl From<WireIn> for Reading {
        fn from(wire: WireIn) -> Self {
            Reading(wire.raw)
        }
    }

    #[bin(little, write_only)]
    struct WireOut {
        raw: u16,
    }

    #[bin(write_only, wire = WireOut)]
    struct Writing(u16);

    impl From<&Writing> for WireOut {
        fn from(value: &Writing) -> Self {
            WireOut { raw: value.0 }
        }
    }

    #[test]
    fn a_read_only_mapped_struct_needs_no_encodable_wire_type() {
        assert_eq!(
            Reading::decode_exact(&[0x34, 0x12]).unwrap(),
            Reading(0x1234)
        );
        assert_eq!(
            Reading::decode_all(&[0x34, 0x12, 0x78, 0x56]).unwrap(),
            [Reading(0x1234), Reading(0x5678)]
        );
    }

    #[test]
    fn a_write_only_mapped_struct_needs_no_decodable_wire_type() {
        assert_eq!(Writing(0x1234).to_bytes().unwrap(), [0x34, 0x12]);
    }
}
