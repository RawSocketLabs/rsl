//! `to_bytes`/`to_canonical_bytes` of a **fixed-length** `#[bin]` message allocate their
//! output once, at its final size (the macro passes `FixedBitLen::BIT_LEN`), and write the
//! same bytes as an un-sized `BitWriter`. Variable-length and write-only messages, which
//! have no `FixedBitLen`, keep the growing buffer.

mod macro_ {
    use bnb::{BitEncode, BitError, BitWriter, FixedBitLen, Sink, bin, u4, u12};

    /// The reference encoding: the same fields through a writer that starts empty.
    fn unsized_bytes<T: BitEncode>(value: &T, canonical: bool) -> Vec<u8> {
        let mut w = BitWriter::with_layout(T::LAYOUT);
        if canonical {
            value.canonical_bit_encode(&mut w).unwrap();
        } else {
            value.bit_encode(&mut w).unwrap();
        }
        w.into_bytes()
    }

    #[bin(big)]
    struct Fixed {
        kind: u16,
        seq: u32,
        src: u64,
        dst: u64,
        digest: [u8; 22],
    }

    #[test]
    fn fixed_message_encodes_into_one_exact_allocation() {
        let msg = Fixed {
            kind: 0x0102,
            seq: 0x0304_0506,
            src: 0x0708_090A_0B0C_0D0E,
            dst: 0x0F10_1112_1314_1516,
            digest: [0xA5; 22],
        };
        let bytes = msg.to_bytes().unwrap();
        assert_eq!(bytes, unsized_bytes(&msg, false));
        assert_eq!(bytes.len(), 44);
        // A buffer that had to grow would hold 64 bytes: 8, 16, 32, 64.
        assert_eq!(bytes.capacity(), bytes.len(), "allocated once, exactly");
    }

    #[bin(little, bits = lsb)]
    struct Canon {
        tag: u4,
        #[reserved]
        spare: u12,
        len: u32,
        #[reserved_with(0xFFFF_FFFF_FFFF_FFFF_u64)]
        ones: u64,
    }

    #[test]
    fn canonical_bytes_are_sized_the_same_way() {
        let msg = Canon {
            tag: u4::new(0x9),
            spare: u12::new(0xABC),
            len: 0x0102_0304,
            ones: 7,
        };
        let verbatim = msg.to_bytes().unwrap();
        assert_eq!(verbatim, unsized_bytes(&msg, false));
        assert_eq!(verbatim.capacity(), 14);
        let canonical = msg.to_canonical_bytes().unwrap();
        assert_eq!(canonical, unsized_bytes(&msg, true));
        assert_ne!(canonical, verbatim);
        assert_eq!(canonical.capacity(), 14);
    }

    // 12 bits: the size rounds up to the two bytes the padded output holds.
    #[bin(big)]
    struct Sub {
        a: u4,
        b: u4,
        c: u4,
    }

    #[test]
    fn a_partial_last_byte_is_counted() {
        assert_eq!(<Sub as FixedBitLen>::BIT_LEN, 12);
        let msg = Sub {
            a: u4::new(1),
            b: u4::new(2),
            c: u4::new(3),
        };
        let bytes = msg.to_bytes().unwrap();
        assert_eq!(bytes, [0x12, 0x30]);
        assert_eq!(bytes.capacity(), 2);
    }

    #[bin(big)]
    struct Variable {
        #[brw(count_prefix = u8)]
        body: Vec<u16>,
        tail: u32,
    }

    #[test]
    fn variable_message_bytes_are_unchanged() {
        let msg = Variable {
            body: (0..40).collect(),
            tail: 0xDEAD_BEEF,
        };
        let bytes = msg.to_bytes().unwrap();
        assert_eq!(bytes, unsized_bytes(&msg, false));
        assert_eq!(bytes.len(), 85);
    }

    /// Encodes, but has no `FixedBitLen`: a write-only parent must not start asking for one.
    struct EncodeOnly(u8);

    impl BitEncode for EncodeOnly {
        fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
            w.write(self.0)
        }
    }

    #[bin(big, write_only)]
    struct WriteOnly {
        head: u16,
        inner: EncodeOnly,
    }

    #[test]
    fn write_only_message_needs_no_fixed_length() {
        let msg = WriteOnly {
            head: 0x0102,
            inner: EncodeOnly(3),
        };
        assert_eq!(msg.to_bytes().unwrap(), [1, 2, 3]);
    }
}
