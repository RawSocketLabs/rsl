//! `#[bitfield]` intercepts a `#[derive(Debug)]` and emits a custom impl that decomposes
//! the **logical** fields (via their getters) instead of the std derive's opaque backing
//! integer (`{ value: 69 }`). Bitfields nested in a `#[bin]` message inherit it.

mod macro_ {

    use bnb::{BitEnum, bin, bitfield, u2, u4, u6};

    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
    #[bit_enum(u2)]
    enum Ecn {
        NotEct,
        Ect1,
        Ect0,
        Ce,
    }

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Tos {
        dscp: u6,
        ecn: Ecn,
    }

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct VersionIhl {
        version: u4,
        ihl: u4,
    }

    #[test]
    fn debug_decomposes_logical_fields_not_the_backing_int() {
        let v = VersionIhl::from_be_bytes([0x45]);
        let s = format!("{v:?}");
        assert!(s.contains("version"), "names the field: {s}");
        assert!(s.contains("ihl"), "names the field: {s}");
        assert!(s.contains('4') && s.contains('5'), "shows the values: {s}");
        assert!(
            !s.contains("value:"),
            "must not show the opaque backing int: {s}"
        );
        assert!(s.starts_with("VersionIhl"), "names the struct: {s}");
    }

    #[test]
    fn debug_shows_a_nested_bitenum_variant() {
        let t = Tos::new().with_dscp(u6::new(46)).with_ecn(Ecn::Ce);
        let s = format!("{t:?}");
        assert!(s.contains("dscp"), "{s}");
        assert!(s.contains("ecn"), "{s}");
        assert!(
            s.contains("Ce"),
            "shows the enum variant, not a raw int: {s}"
        );
    }

    // A bitfield nested in a `#[bin]` message: the message's std-derived `Debug` calls the
    // bitfield's custom `Debug`, so the logical fields show through.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Header {
        ver_ihl: VersionIhl,
        tos: Tos,
    }

    #[test]
    fn nested_in_bin_message_inherits_the_logical_debug() {
        let h = Header::decode_exact(&[0x45, 0x00]).unwrap();
        let s = format!("{h:?}");
        assert!(s.contains("version"), "the nested bitfield decomposes: {s}");
        assert!(!s.contains("value:"), "no opaque backing int: {s}");
    }
}
