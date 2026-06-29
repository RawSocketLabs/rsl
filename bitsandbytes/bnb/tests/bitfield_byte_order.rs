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
}
