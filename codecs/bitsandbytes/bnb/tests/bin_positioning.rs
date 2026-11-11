//! Positioning: forward `pad_*`/`align_*` directives skip bits or
//! align to a byte boundary around a field, with typed amounts from `bnb::prelude`
//! (`4.bits()`, `1.bytes()`). Backward `seek`/`restore_position` need `SeekSource`.

mod macro_ {

    use bnb::prelude::*;
    use bnb::{BitReader, bin, u4};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        tag: u4,
        #[br(pad_before = 4.bits())] // 4 reserved bits between tag and value
        value: u8,
        #[br(align_after)] // pad to a byte boundary after the (here already-aligned) field
        trailer: u4,
    }

    #[test]
    fn pad_and_align_round_trip() {
        let f = Frame {
            tag: u4::new(0x5),
            value: 0xAB,
            trailer: u4::new(0x3),
        };
        let bytes = f.to_bytes().unwrap();
        // tag(0101) pad(0000) | value(0xAB) | trailer(0011) pad-to-byte(0000)
        assert_eq!(bytes, [0x50, 0xAB, 0x30]);
        assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    }

    // A whole-byte pad expressed in bytes.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Spaced {
        a: u4,
        #[br(pad_before = 1.bytes())] // skip a full byte
        b: u4,
    }

    #[test]
    fn byte_sized_pad() {
        let f = Spaced {
            a: u4::new(0xA),
            b: u4::new(0xB),
        };
        let bytes = f.to_bytes().unwrap();
        // a(1010) pad-hi(0000) | pad-lo(0000 0000)? -> a at 0..4, pad 4..12, b 12..16
        // byte0 = 1010_0000, byte1 = 0000_1011
        assert_eq!(bytes, [0xA0, 0x0B]);
        assert_eq!(Spaced::decode_exact(&bytes).unwrap(), f);
    }

    // ── Each directive moves the cursor in *both* directions ─────────────────────────────
    // The decode inputs below carry `1` bits in the padding: a decoder that forgot the
    // directive would read them as the next field (or stop short of them), so the following
    // field's value and the final cursor position are what prove the skip happened.

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct PadAfter {
        #[br(pad_after = 12.bits())] // 12 reserved bits between tag and value
        tag: u4,
        value: u8,
    }

    #[test]
    fn pad_after_is_written_as_zero_bits_before_the_next_field() {
        let f = PadAfter {
            tag: u4::new(0x5),
            value: 0xAB,
        };
        // tag(0101) pad(0000 | 0000 0000) | value(0xAB)
        assert_eq!(f.to_bytes().unwrap(), [0x50, 0x00, 0xAB]);
    }

    #[test]
    fn pad_after_is_skipped_on_decode_so_the_next_field_reads_its_own_bytes() {
        let f = PadAfter::decode_exact(&[0x5F, 0xFF, 0xAB]).unwrap();
        assert_eq!(f.tag, u4::new(0x5));
        assert_eq!(f.value, 0xAB, "value must come from after the 12 pad bits");
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct AlignAfter {
        #[br(align_after)] // pad the 4-bit tag out to the byte boundary
        tag: u4,
        value: u8,
    }

    #[test]
    fn align_after_pads_the_field_to_a_byte_boundary_on_encode() {
        let f = AlignAfter {
            tag: u4::new(0x5),
            value: 0xAB,
        };
        // tag(0101) align(0000) | value(0xAB)
        assert_eq!(f.to_bytes().unwrap(), [0x50, 0xAB]);
    }

    #[test]
    fn align_after_is_skipped_on_decode_so_the_next_field_starts_byte_aligned() {
        let f = AlignAfter::decode_exact(&[0x5F, 0xAB]).unwrap();
        assert_eq!(f.tag, u4::new(0x5));
        assert_eq!(f.value, 0xAB, "value must start at the byte boundary");
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct AlignBefore {
        tag: u4,
        #[br(align_before)] // realign before the byte-wide value
        value: u8,
    }

    #[test]
    fn align_before_pads_to_a_byte_boundary_ahead_of_the_field_on_encode() {
        let f = AlignBefore {
            tag: u4::new(0x5),
            value: 0xAB,
        };
        // tag(0101) align(0000) | value(0xAB)
        assert_eq!(f.to_bytes().unwrap(), [0x50, 0xAB]);
    }

    #[test]
    fn align_before_is_skipped_on_decode_so_the_field_starts_byte_aligned() {
        let f = AlignBefore::decode_exact(&[0x5F, 0xAB]).unwrap();
        assert_eq!(f.tag, u4::new(0x5));
        assert_eq!(f.value, 0xAB, "value must start at the byte boundary");
    }

    #[test]
    fn pad_before_is_skipped_on_decode_even_when_the_pad_bits_are_set() {
        // `Frame`: tag(0101) pad(1111) | value(0xAB) | trailer(0011) align(0000)
        let f = Frame::decode_exact(&[0x5F, 0xAB, 0x30]).unwrap();
        assert_eq!(f.tag, u4::new(0x5));
        assert_eq!(f.value, 0xAB, "value must come from after the 4 pad bits");
    }

    // A directive on the *last* field has no following field to misread, so the cursor is
    // the only witness: the decoder must leave it past the padding.

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct TrailingPad {
        value: u8,
        #[br(pad_after = 2.bytes())] // a fixed 4-byte footprint
        crc: u8,
    }

    #[test]
    fn trailing_pad_after_is_part_of_the_encoded_footprint() {
        let f = TrailingPad {
            value: 0xAB,
            crc: 0xC7,
        };
        assert_eq!(f.to_bytes().unwrap(), [0xAB, 0xC7, 0x00, 0x00]);
    }

    #[test]
    fn trailing_pad_after_leaves_the_decode_cursor_past_the_padding() {
        let bytes = [0xAB, 0xC7, 0xFF, 0xFF, 0x77];
        let mut r = BitReader::new(&bytes);
        let f = TrailingPad::decode(&mut r).unwrap();
        assert_eq!(
            f,
            TrailingPad {
                value: 0xAB,
                crc: 0xC7
            }
        );
        assert_eq!(r.bit_pos(), 32, "the two pad bytes belong to the message");
        assert_eq!(r.read::<u8>().unwrap(), 0x77);
    }

    #[test]
    fn trailing_align_after_leaves_the_decode_cursor_on_the_byte_boundary() {
        // `Frame` ends tag|pad|value|trailer(4 bits) + `align_after`: 20 bits read, 24 consumed.
        let bytes = [0x50, 0xAB, 0x3F, 0x77];
        let mut r = BitReader::new(&bytes);
        let f = Frame::decode(&mut r).unwrap();
        assert_eq!(f.trailer, u4::new(0x3));
        assert_eq!(
            r.bit_pos(),
            24,
            "the 4 alignment bits belong to the message"
        );
        assert_eq!(r.read::<u8>().unwrap(), 0x77);
    }

    // Enum variant fields go through their own encode path; the directive must hold there too.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    enum Tagged {
        #[bin(magic = 1u8)]
        Padded {
            #[br(pad_after = 1.bytes())]
            kind: u8,
            #[br(align_after)]
            nibble: u4,
            value: u8,
        },
    }

    #[test]
    fn variant_fields_honor_pad_after_and_align_after_in_both_directions() {
        let v = Tagged::Padded {
            kind: 0x22,
            nibble: u4::new(0x9),
            value: 0xAB,
        };
        // magic | kind | pad byte | nibble(1001) align(0000) | value
        assert_eq!(v.to_bytes().unwrap(), [0x01, 0x22, 0x00, 0x90, 0xAB]);
        assert_eq!(
            Tagged::decode_exact(&[0x01, 0x22, 0xFF, 0x9F, 0xAB]).unwrap(),
            v,
            "set pad/alignment bits are discarded, not read as fields"
        );
    }
}
