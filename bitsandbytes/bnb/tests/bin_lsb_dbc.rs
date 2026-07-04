//! The **LSB × byte-order rule**, pinned against the DBC/"Intel" reference semantics
//! (CAN databases, and the same layout SMB-style LSB formats use):
//!
//! > An Intel signal of length `L` at start-bit `S` occupies `raw |= value << S` of a
//! > **little-endian** integer; the frame bytes are `raw.to_le_bytes()`.
//!
//! Each bit order has a *natural* byte layout — MSB-first emits high bits first (bytes land
//! big-endian), LSB-first emits low bits first (bytes land little-endian). The byte-order
//! knob swaps a byte-multiple value only when it *differs* from that natural layout. So
//! `#[bin(little, bit_order = lsb)]` — the intuitive "Intel" declaration — **is** the DBC
//! layout, byte-identically. The reference formula is embedded here (not a tool's output),
//! so the assertion is against the *specified* semantics.

mod macro_ {
    use bnb::{bin, u3, u4};

    /// The DBC-Intel reference: sig_a `u3`@0, sig_b `u4`@3, word `u16`@7 (byte-multiple,
    /// byte-straddling — the case the rule governs), tail `u3`@23. 26 bits → 4 bytes.
    fn dbc_reference(a: u64, b: u64, word: u64, tail: u64) -> [u8; 4] {
        let raw: u64 = a | (b << 3) | (word << 7) | (tail << 23);
        let le = raw.to_le_bytes();
        [le[0], le[1], le[2], le[3]]
    }

    #[bin(little, bit_order = lsb)]
    #[derive(Debug, PartialEq, Clone)]
    struct Intel {
        a: u3,
        b: u4,
        word: u16,
        tail: u3,
    }

    #[bin(big, bit_order = lsb)]
    #[derive(Debug, PartialEq, Clone)]
    struct LsbSwapped {
        a: u3,
        b: u4,
        word: u16,
        tail: u3,
    }

    fn intel(a: u8, b: u8, word: u16, tail: u8) -> Intel {
        Intel {
            a: u3::new(a),
            b: u4::new(b),
            word,
            tail: u3::new(tail),
        }
    }

    #[test]
    fn lsb_little_matches_the_dbc_intel_reference() {
        let v = intel(0b101, 0b1100, 0x1234, 0b011);
        assert_eq!(
            v.to_bytes().unwrap(),
            dbc_reference(0b101, 0b1100, 0x1234, 0b011)
        );
        // And across a value sweep, not just one point.
        for (a, b, word, tail) in [
            (0, 0, 0, 0),
            (7, 15, u16::MAX, 7),
            (1, 8, 0x00FF, 4),
            (6, 3, 0xFF00, 1),
            (2, 9, 0xABCD, 5),
        ] {
            let v = intel(a, b, word, tail);
            assert_eq!(
                v.to_bytes().unwrap(),
                dbc_reference(a as u64, b as u64, word as u64, tail as u64),
                "a={a} b={b} word={word:#06x} tail={tail}",
            );
            // Decode is the exact inverse of the reference layout too.
            assert_eq!(Intel::decode_exact(&v.to_bytes().unwrap()).unwrap(), v);
        }
    }

    #[test]
    fn lsb_big_is_the_deliberate_swap() {
        // `big` under LSB differs from the natural layout, so the u16's bytes swap
        // relative to the DBC layout; the sub-byte fields are untouched.
        let little = intel(0b101, 0b1100, 0x1234, 0b011).to_bytes().unwrap();
        let big = LsbSwapped {
            a: u3::new(0b101),
            b: u4::new(0b1100),
            word: 0x1234,
            tail: u3::new(0b011),
        }
        .to_bytes()
        .unwrap();
        assert_ne!(
            little, big,
            "the byte-order knob must be observable under lsb"
        );
        assert_eq!(
            little[0], big[0],
            "sub-byte prefix unaffected by byte order"
        );
        // Round-trips in its own layout, like every corner.
        let v = LsbSwapped {
            a: u3::new(1),
            b: u4::new(2),
            word: 0xBEEF,
            tail: u3::new(3),
        };
        assert_eq!(LsbSwapped::decode_exact(&v.to_bytes().unwrap()).unwrap(), v);
    }

    /// A pure byte-multiple message under `lsb`+`little` lays out like `to_le_bytes` —
    /// the degenerate (no sub-byte fields) corner of the same rule.
    #[test]
    fn lsb_little_byte_multiple_is_plain_little_endian() {
        #[bin(little, bit_order = lsb)]
        #[derive(Debug, PartialEq)]
        struct Words {
            x: u16,
            y: u32,
        }
        let w = Words {
            x: 0x1122,
            y: 0xAABBCCDD,
        };
        assert_eq!(w.to_bytes().unwrap(), [0x22, 0x11, 0xDD, 0xCC, 0xBB, 0xAA]);
    }
}

mod property {
    use bnb::{bin, u3, u4};
    use proptest::prelude::*;

    #[bin(little, bit_order = lsb)]
    #[derive(Debug, PartialEq, Clone)]
    struct Intel {
        a: u3,
        b: u4,
        word: u16,
        tail: u3,
    }

    proptest! {
        /// Every value agrees with the DBC-Intel reference formula, and round-trips.
        #[test]
        fn lsb_little_equals_dbc_for_all_values(a in 0u8..8, b in 0u8..16, word in any::<u16>(), tail in 0u8..8) {
            let v = Intel { a: u3::new(a), b: u4::new(b), word, tail: u3::new(tail) };
            let bytes = v.to_bytes().unwrap();
            let raw: u64 = (a as u64) | ((b as u64) << 3) | ((word as u64) << 7) | ((tail as u64) << 23);
            prop_assert_eq!(&bytes[..], &raw.to_le_bytes()[..4]);
            prop_assert_eq!(Intel::decode_exact(&bytes).unwrap(), v);
        }
    }
}
