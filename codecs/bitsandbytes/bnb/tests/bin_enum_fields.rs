//! Variant fields of a `#[bin]` enum take the struct field directives through the **same**
//! write generator: one round-trip per directive, asserting the exact bytes. An enum encodes
//! verbatim (it has no canonical form), so a stored `reserved` value is written as stored and
//! `decode → to_bytes` is byte-identical. (A stored `calc` would never run, so it is rejected:
//! `tests/ui/bin_enum_variant_stored_calc.rs`.)

mod macro_ {
    use bnb::prelude::*;
    use bnb::{WireLen, bin, u4};

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Celsius(i16);

    // A variable-length nested message, for the `bytes(..)` probe.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Inner {
        #[brw(count_prefix = u8)]
        data: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Field {
        #[bin(magic = 0x01u8)]
        If {
            flag: u8,
            #[br(if(flag != 0))]
            extra: Option<u16>,
        },
        #[bin(magic = 0x03u8)]
        TempCalc {
            #[br(temp)]
            #[bw(calc = u8::try_from(body.len()).unwrap())]
            len: u8,
            #[br(count = len)]
            body: Vec<u8>,
        },
        #[bin(magic = 0x04u8)]
        BrCalc {
            raw: u8,
            #[br(calc = raw + 1)]
            next: u8,
        },
        #[bin(magic = 0x05u8)]
        Positioned {
            tag: u4,
            #[br(pad_before = 4.bits())]
            value: u8,
            #[br(align_after)]
            trailer: u4,
            #[br(pad_after = 1.bytes())]
            last: u8,
        },
        #[bin(magic = 0x06u8)]
        Seek {
            ptr: u8,
            #[br(seek = ptr.bytes(), restore_position)]
            target: u8,
            next: u8,
        },
        #[bin(magic = 0x07u8)]
        Array([u8; 3]),
        #[bin(magic = 0x08u8)]
        Mapped(
            #[br(map = |raw: u16| Celsius(i16::from_ne_bytes(raw.to_ne_bytes())))]
            #[bw(map = |c: &Celsius| u16::from_ne_bytes(c.0.to_ne_bytes()))]
            Celsius,
        ),
        #[bin(magic = 0x09u8)]
        Counted {
            n: u8,
            #[br(count = n)]
            items: Vec<u16>,
        },
        #[bin(magic = 0x0Au8)]
        Reserved {
            #[reserved]
            rsv: u8,
            value: u8,
        },
        #[bin(magic = 0x0Bu8)]
        AutoCount {
            #[bw(auto_len = count(items))]
            len: WireLen<u16>,
            #[br(count = len.to_count())]
            items: Vec<u8>,
        },
        #[bin(magic = 0x0Cu8)]
        AutoBytes {
            #[bw(auto_len = bytes(inner))]
            nbytes: WireLen<u16>,
            #[brw(variable)]
            inner: Inner,
        },
        #[bin(magic = 0x0Du8)]
        Codec {
            #[br(parse_with = bnb::codecs::leb128::parse)]
            #[bw(write_with = bnb::codecs::leb128::write)]
            size: u64,
        },
        #[bin(magic = 0x0Eu8)]
        Ignored {
            value: u8,
            #[brw(ignore)]
            note: u8,
        },
    }

    /// `value` encodes to exactly `bytes`, and `bytes` decodes back to `value`.
    fn round_trip(value: &Field, bytes: &[u8]) {
        assert_eq!(value.to_bytes().unwrap(), bytes);
        assert_eq!(&Field::decode_exact(bytes).unwrap(), value);
    }

    #[test]
    fn variant_if_writes_the_inner_value_only_when_present() {
        round_trip(
            &Field::If {
                flag: 1,
                extra: Some(0xBEEF),
            },
            &[0x01, 0x01, 0xBE, 0xEF],
        );
        round_trip(
            &Field::If {
                flag: 0,
                extra: None,
            },
            &[0x01, 0x00],
        );
    }

    #[test]
    fn variant_temp_calc_is_recomputed_on_encode() {
        round_trip(
            &Field::TempCalc {
                body: vec![0xAA, 0xBB, 0xCC],
            },
            &[0x03, 0x03, 0xAA, 0xBB, 0xCC],
        );
    }

    #[test]
    fn variant_br_calc_writes_nothing() {
        // `next` is computed on decode from `raw`; its bits are `raw`'s, so it is not written.
        round_trip(&Field::BrCalc { raw: 5, next: 6 }, &[0x04, 0x05]);
    }

    #[test]
    fn variant_positioning_pads_and_aligns() {
        // tag(0101) pad(0000) | value | trailer(0011) align(0000) | last | pad byte
        round_trip(
            &Field::Positioned {
                tag: u4::new(0x5),
                value: 0xAB,
                trailer: u4::new(0x3),
                last: 0x77,
            },
            &[0x05, 0x50, 0xAB, 0x30, 0x77, 0x00],
        );
    }

    #[test]
    fn variant_seek_restore_position_is_read_only() {
        // `target` peeks the byte at offset `ptr` (here `next`'s own byte) and is not written.
        round_trip(
            &Field::Seek {
                ptr: 2,
                target: 0x11,
                next: 0x11,
            },
            &[0x06, 0x02, 0x11],
        );
    }

    #[test]
    fn variant_byte_array_round_trips() {
        round_trip(&Field::Array([1, 2, 3]), &[0x07, 1, 2, 3]);
    }

    #[test]
    fn variant_map_writes_through_the_inverse() {
        round_trip(&Field::Mapped(Celsius(-2)), &[0x08, 0xFF, 0xFE]);
    }

    #[test]
    fn variant_count_reads_the_sibling_length() {
        round_trip(
            &Field::Counted {
                n: 2,
                items: vec![0x0102, 0x0304],
            },
            &[0x09, 0x02, 0x01, 0x02, 0x03, 0x04],
        );
    }

    #[test]
    fn variant_reserved_is_written_as_stored() {
        // Verbatim: a non-zero reserved value is kept (an enum has no canonical encode).
        round_trip(
            &Field::Reserved {
                rsv: 0x7F,
                value: 0x01,
            },
            &[0x0A, 0x7F, 0x01],
        );
    }

    #[test]
    fn variant_auto_len_count_derives_and_honors_a_set_override() {
        let auto = Field::AutoCount {
            len: WireLen::auto(),
            items: vec![0xAA, 0xBB, 0xCC],
        };
        let bytes = [0x0B, 0x00, 0x03, 0xAA, 0xBB, 0xCC];
        assert_eq!(auto.to_bytes().unwrap(), bytes);
        // Decode yields `Set(3)`, which round-trips byte-identically.
        round_trip(
            &Field::AutoCount {
                len: WireLen::set(3),
                items: vec![0xAA, 0xBB, 0xCC],
            },
            &bytes,
        );
        // A forged length is written as given.
        let forged = Field::AutoCount {
            len: WireLen::set(1),
            items: vec![0xAA, 0xBB],
        };
        assert_eq!(forged.to_bytes().unwrap(), [0x0B, 0x00, 0x01, 0xAA, 0xBB]);
    }

    #[test]
    fn variant_auto_len_bytes_measures_the_encoded_sibling() {
        let auto = Field::AutoBytes {
            nbytes: WireLen::auto(),
            inner: Inner {
                data: vec![0x09, 0x09],
            },
        };
        // `inner` encodes as [len=2, 9, 9] = 3 bytes.
        let bytes = [0x0C, 0x00, 0x03, 0x02, 0x09, 0x09];
        assert_eq!(auto.to_bytes().unwrap(), bytes);
        round_trip(
            &Field::AutoBytes {
                nbytes: WireLen::set(3),
                inner: Inner {
                    data: vec![0x09, 0x09],
                },
            },
            &bytes,
        );
    }

    #[test]
    fn variant_parse_with_write_with_uses_the_custom_codec() {
        round_trip(&Field::Codec { size: 300 }, &[0x0D, 0xAC, 0x02]);
    }

    #[test]
    fn variant_ignore_is_not_written_and_defaults_on_decode() {
        let v = Field::Ignored { value: 5, note: 9 };
        assert_eq!(v.to_bytes().unwrap(), [0x0E, 0x05]);
        assert_eq!(
            Field::decode_exact(&[0x0E, 0x05]).unwrap(),
            Field::Ignored { value: 5, note: 0 }
        );
    }
}
