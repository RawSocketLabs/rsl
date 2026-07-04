//! `#[bitfield]` standalone serialization honors the **declared** `bytes = be|le`:
//! `to_bytes`/`from_bytes` use it, while the endianness-explicit
//! `to_be_bytes`/`to_le_bytes` ignore it (an override). Same logical value, declared two
//! ways, gives two wire encodings — the proof that the byte-order knob is observable.

mod macro_ {

    use bnb::{bitfield, u4};

    #[bitfield(u16, bits = msb, bytes = be)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Be {
        hi: u4,
        mid: u8,
        lo: u4,
    }

    #[bitfield(u16, bits = msb, bytes = le)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Le {
        hi: u4,
        mid: u8,
        lo: u4,
    }

    fn be() -> Be {
        Be::new()
            .with_hi(u4::new(0xA))
            .with_mid(0xBC)
            .with_lo(u4::new(0xD))
    }
    fn le() -> Le {
        Le::new()
            .with_hi(u4::new(0xA))
            .with_mid(0xBC)
            .with_lo(u4::new(0xD))
    }

    #[test]
    fn to_bytes_uses_the_declared_big_order() {
        assert_eq!(be().to_bytes(), [0xAB, 0xCD]);
    }

    #[test]
    fn to_bytes_uses_the_declared_little_order() {
        // Same logical value (0xABCD) as `Be`, but declared little-endian.
        assert_eq!(le().to_bytes(), [0xCD, 0xAB]);
    }

    #[test]
    fn from_bytes_inverts_to_bytes_in_the_declared_order() {
        assert_eq!(Be::from_bytes(be().to_bytes()), be());
        assert_eq!(Le::from_bytes(le().to_bytes()), le());
    }

    #[test]
    fn explicit_endian_methods_ignore_the_declaration() {
        // to_be_bytes/to_le_bytes are the override: they emit the named endianness
        // regardless of the struct's declared `bytes =`.
        assert_eq!(le().to_be_bytes(), [0xAB, 0xCD]);
        assert_eq!(be().to_le_bytes(), [0xCD, 0xAB]);
        // …and their from_* counterparts likewise.
        assert_eq!(Be::from_le_bytes([0xCD, 0xAB]), be());
        assert_eq!(Le::from_be_bytes([0xAB, 0xCD]), le());
    }

    // The two bitfield axes — `bits` (packing into the backing integer) and `bytes` (how that
    // integer serializes) — compose independently. `bits = lsb` puts the first field in the LOW
    // bits, so the same logical fields pack to a different `raw`; `to_bytes` then serializes that
    // `raw` in the declared byte order.
    #[bitfield(u16, bits = lsb, bytes = be)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct LsbBe {
        hi: u4,
        mid: u8,
        lo: u4,
    }
    #[bitfield(u16, bits = lsb, bytes = le)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct LsbLe {
        hi: u4,
        mid: u8,
        lo: u4,
    }

    #[test]
    fn bit_and_byte_order_compose_independently() {
        // msb packing -> raw 0xABCD; lsb packing (first field low) -> raw 0xDBCA.
        assert_eq!(be().raw(), 0xABCD);
        let lsb = LsbBe::new()
            .with_hi(u4::new(0xA))
            .with_mid(0xBC)
            .with_lo(u4::new(0xD));
        assert_eq!(lsb.raw(), 0xDBCA);

        // The four (bits × bytes) corners give four distinct wire encodings via `to_bytes`.
        let lsb_le = LsbLe::new()
            .with_hi(u4::new(0xA))
            .with_mid(0xBC)
            .with_lo(u4::new(0xD));
        assert_eq!(be().to_bytes(), [0xAB, 0xCD]); // msb / be
        assert_eq!(le().to_bytes(), [0xCD, 0xAB]); // msb / le
        assert_eq!(lsb.to_bytes(), [0xDB, 0xCA]); // lsb / be
        assert_eq!(lsb_le.to_bytes(), [0xCA, 0xDB]); // lsb / le
    }
}
