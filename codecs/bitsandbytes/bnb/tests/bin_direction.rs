//! `read_only` / `write_only` narrow a `#[bin]` type to one direction, and a narrowed type
//! needs nothing from the other one: no write inverse for a read-side directive, no `count`
//! for a `Vec` that is only written, and no write-side impl on a wire type that is only
//! read.

mod macro_ {
    use bnb::{BitEncode, BitError, BitOrder, ByteOrder, Layout, Sink, bin};

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

    // ── A read-only mapped struct is itself a read-only wire type: a second mapped struct
    // over it decodes in the innermost wire type's order. ──
    #[bin(read_only, wire = Reading)]
    #[derive(Debug, PartialEq)]
    struct Chained(u16);

    impl From<Reading> for Chained {
        fn from(inner: Reading) -> Self {
            Chained(inner.0)
        }
    }

    #[test]
    fn a_read_only_mapped_struct_can_be_a_wire_type() {
        assert_eq!(
            Chained::decode_exact(&[0x34, 0x12]).unwrap(),
            Chained(0x1234)
        );
    }

    // ── The other two sources of a decode-only mapped struct's layout: a wire type that
    // also encodes (its `BitEncode::LAYOUT`), and the closure form, which is decode-only
    // for want of a `bw_map` rather than by `read_only`. Little-endian, so a default
    // layout would show. ──
    #[bin(little)]
    struct WireBoth {
        raw: u16,
    }

    #[bin(read_only, wire = WireBoth)]
    #[derive(Debug, PartialEq)]
    struct ReadingBoth(u16);

    impl From<WireBoth> for ReadingBoth {
        fn from(wire: WireBoth) -> Self {
            ReadingBoth(wire.raw)
        }
    }

    #[bin(map = |wire: WireIn| ReadingClosure(wire.raw))]
    #[derive(Debug, PartialEq)]
    struct ReadingClosure(u16);

    #[test]
    fn a_decode_only_mapped_struct_reads_in_an_encodable_wire_types_order() {
        assert_eq!(
            ReadingBoth::decode_exact(&[0x34, 0x12]).unwrap(),
            ReadingBoth(0x1234)
        );
    }

    #[test]
    fn a_closure_form_mapped_struct_without_bw_map_is_decode_only() {
        assert_eq!(
            ReadingClosure::decode_exact(&[0x34, 0x12]).unwrap(),
            ReadingClosure(0x1234)
        );
    }

    // ── A read-only tag enum (selector, catch-all, read-side directives with no inverse)
    // and a write-only enum with a catch-all (a `Vec` with no `count`, a write-side
    // directive with no inverse). ──
    #[bin(big, read_only, ctx(kind: u8), tag = kind)]
    #[derive(Debug, PartialEq)]
    enum TaggedIn {
        #[bin(tag = 1)]
        Mapped(#[br(map = |raw: u8| u16::from(raw) * 2)] u16),
        #[bin(tag = 2)]
        Counted {
            #[br(temp)]
            len: u8,
            #[br(count = len)]
            body: Vec<u8>,
        },
        #[catch_all]
        Other(u8, u8),
    }

    #[bin(big, write_only)]
    enum CatchAllOut {
        #[bin(magic = 1u8)]
        Run { body: Vec<u8> },
        #[catch_all]
        Other(u8, #[bw(write_with = bnb::codecs::leb128::write)] u64),
    }

    #[test]
    fn a_read_only_tag_enum_needs_no_write_inverses() {
        let decode =
            |bytes: &[u8], kind| TaggedIn::decode_with_exact(bytes, TaggedInCtx { kind }).unwrap();
        assert_eq!(decode(&[21], 1), TaggedIn::Mapped(42));
        assert_eq!(
            decode(&[2, 0xAA, 0xBB], 2),
            TaggedIn::Counted {
                body: vec![0xAA, 0xBB]
            }
        );
        assert_eq!(decode(&[7], 9), TaggedIn::Other(9, 7));
    }

    #[test]
    fn a_write_only_catch_all_enum_needs_nothing_read_side() {
        let run = CatchAllOut::Run {
            body: vec![0xAA, 0xBB],
        };
        assert_eq!(run.to_bytes().unwrap(), [1, 0xAA, 0xBB]);
        assert_eq!(
            CatchAllOut::Other(9, 128).to_bytes().unwrap(),
            [9, 0x80, 0x01]
        );
    }

    // ── A read-only wire type's own order is the one it is decoded in, whatever a
    // hand-written `BitEncode` beside it declares. ──
    #[bin(little, read_only)]
    #[derive(Debug, PartialEq)]
    struct OwnOrder {
        raw: u16,
    }

    impl BitEncode for OwnOrder {
        const LAYOUT: Layout = Layout {
            bit: BitOrder::Msb,
            byte: ByteOrder::Big,
        };

        fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
            w.write(self.raw)
        }
    }

    #[bin(read_only, wire = OwnOrder)]
    #[derive(Debug, PartialEq)]
    struct ViaOwnOrder(u16);

    impl From<OwnOrder> for ViaOwnOrder {
        fn from(wire: OwnOrder) -> Self {
            ViaOwnOrder(wire.raw)
        }
    }

    #[test]
    fn a_mapped_struct_decodes_in_its_read_only_wire_types_own_order() {
        let bytes = [0x34, 0x12];
        assert_eq!(
            OwnOrder::decode_exact(&bytes).unwrap(),
            OwnOrder { raw: 0x1234 }
        );
        assert_eq!(
            ViaOwnOrder::decode_exact(&bytes).unwrap(),
            ViaOwnOrder(0x1234)
        );
    }
}
