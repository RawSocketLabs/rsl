//! `#[bin]` foundation: one attribute macro folds the
//! codec and the required-by-default builder (generated directly, not by lowering
//! to the derives). Field directives (`#[br]`/`#[bw]`) are covered by the other
//! `bin_*.rs` files.

mod macro_ {

    use bnb::{BuilderError, bin, u4, u12};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Frame {
        version: u4,
        #[builder(default)]
        flags: u4,
        payload_len: u12,
    }

    #[test]
    fn folds_codec_and_builder() {
        let f = Frame::builder()
            .version(u4::new(4))
            .payload_len(u12::new(100))
            .build()
            .unwrap();
        assert_eq!(f.flags, u4::new(0), "builder default");

        let bytes = f.to_bytes().unwrap();
        assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
    }

    #[test]
    fn enforces_required_fields() {
        let err = Frame::builder().version(u4::new(4)).build().unwrap_err();
        assert_eq!(err, BuilderError::MissingField("payload_len"));
    }

    #[bin(bits = lsb)]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct LsbWord {
        a: u4,
        b: u12,
    }

    #[test]
    fn bit_order_option_lowers() {
        let v = LsbWord {
            a: u4::new(0xA),
            b: u12::new(0),
        };
        // LSB-first packs the first field into the low bits.
        assert_eq!(v.to_bytes().unwrap()[0] & 0x0F, 0xA);
    }

    #[bin(read_only)]
    #[derive(Debug, PartialEq, Eq)]
    struct ReadOnly {
        a: u4,
        b: u12,
    }

    #[test]
    fn read_only_decodes_only() {
        let r = ReadOnly::peek(&[0xAB, 0xCD]).unwrap();
        assert_eq!(
            r,
            ReadOnly {
                a: u4::new(0xA),
                b: u12::new(0xBCD)
            }
        );
        // No `to_bytes`/builder is generated for a read_only type (enforced at compile
        // time — only Decode is derived).
    }

    // `no_builder` is true of an item kind that has no builder: accepted as a no-op, so a
    // wrapper macro can stamp `#[bin(big, no_builder)]` on every item it emits.
    #[bin(big, no_builder)]
    #[derive(Debug, PartialEq, Eq)]
    enum NoBuilderEnum {
        #[bin(magic = 1u8)]
        Only(u8),
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct NoBuilderWire {
        a: u16,
    }

    #[bin(
        no_builder,
        map = |w: NoBuilderWire| NoBuilderMapped(w.a),
        bw_map = |m: &NoBuilderMapped| NoBuilderWire { a: m.0 }
    )]
    #[derive(Debug, PartialEq, Eq)]
    struct NoBuilderMapped(u16);

    #[bin(no_builder, codec = bnb::codecs::leb128)]
    #[derive(Debug, PartialEq, Eq)]
    struct NoBuilderCodec(u64);

    #[test]
    fn no_builder_is_a_no_op_on_items_without_a_builder() {
        let only = NoBuilderEnum::Only(2);
        assert_eq!(only.to_bytes().unwrap(), [1, 2]);
        assert_eq!(NoBuilderEnum::decode_exact(&[1, 2]).unwrap(), only);

        let mapped = NoBuilderMapped(5);
        assert_eq!(mapped.to_bytes().unwrap(), [0, 5]);
        assert_eq!(NoBuilderMapped::decode_exact(&[0, 5]).unwrap(), mapped);

        let codec = NoBuilderCodec(128);
        assert_eq!(codec.to_bytes().unwrap(), [0x80, 1]);
        assert_eq!(NoBuilderCodec::decode_exact(&[0x80, 1]).unwrap(), codec);
    }

    // Fields named `r`/`w` once collided with the codec's generated source/sink and were
    // a hard error. The params are now `__bnb_r`/`__bnb_w`, so `r`/`w` are ordinary field
    // names — even usable as a sibling local in a directive like `count`.
    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct Rw {
        r: u8,
        w: u8,
        #[br(count = r as usize)]
        tail: Vec<u8>,
    }

    #[test]
    fn fields_named_r_and_w_roundtrip() {
        let v = Rw::builder().r(3).w(7).tail(vec![1, 2, 3]).build().unwrap();
        let bytes = v.to_bytes().unwrap();
        assert_eq!(bytes, vec![3, 7, 1, 2, 3]);
        assert_eq!(Rw::decode_exact(&bytes).unwrap(), v);
    }
}
