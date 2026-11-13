//! `to_bytes`/`to_canonical_bytes` of a **fixed-length** `#[bin]` message allocate their
//! output once, at its final size (the macro passes `FixedBitLen::BIT_LEN`), and write the
//! same bytes as an un-sized `BitWriter`. A variable-length message, or a lone
//! `#[derive(BitEncode)]`, has no `FixedBitLen`: its `to_bytes` reserves a size summed
//! from its fields (a `Vec`'s length times its element's width), which is exact whenever
//! every field's width is visible, and only a starting point otherwise.

// A consumer may forbid a lint outright. The size hint brings a trait into scope that a
// message whose fields all have a fixed width never uses (`Variable` below): generated code
// must neither trip this lint nor try to `allow` it, which `forbid` makes a hard error.
#![forbid(unused_imports)]

mod macro_ {
    use bnb::{BitEncode, BitError, BitWriter, FixedBitLen, Sink, bin, u4, u12};

    /// `value.to_bytes()`, checked against the un-sized reference; returns the bytes.
    macro_rules! sized {
        ($value:expr) => {{
            let value = $value;
            let bytes = value.to_bytes().unwrap();
            assert_eq!(bytes, unsized_bytes(&value, false));
            bytes
        }};
    }

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
        // The count prefix, forty two-byte elements and the tail, reserved before writing;
        // a buffer that grew would hold 128.
        assert_eq!(bytes.capacity(), 85, "allocated once, exactly");
    }

    #[bin(big)]
    #[derive(Clone)]
    struct Record {
        kind: u16,
        seq: u32,
    }

    #[bin(big, magic = 0x4844u16)]
    struct Mixed {
        #[brw(count_prefix = u32)]
        blob: Vec<u8>,
        digest: [u8; 5],
        #[brw(count_prefix = u16)]
        records: Vec<Record>,
        // Not on the wire, so not in the size.
        #[brw(ignore)]
        #[expect(dead_code, reason = "present on the struct, never encoded")]
        scratch: u64,
    }

    #[test]
    fn a_variable_message_reserves_every_field_it_can_see() {
        for (blob, records) in [(0, 0), (1, 1), (1000, 3), (7, 300)] {
            let bytes = sized!(Mixed {
                blob: vec![0x5A; blob],
                digest: [1, 2, 3, 4, 5],
                records: vec![Record { kind: 7, seq: 9 }; records],
                scratch: 0,
            });
            // Magic, both prefixes, the digest, then the runs.
            assert_eq!(bytes.len(), 2 + 4 + 5 + 2 + blob + 6 * records);
            assert_eq!(
                bytes.capacity(),
                bytes.len(),
                "{blob} bytes, {records} records"
            );
        }
    }

    // Twenty bits and a byte run: no `FixedBitLen`, and a partial byte before the run.
    #[derive(BitEncode)]
    struct Bare {
        a: u4,
        b: u16,
        run: Vec<u8>,
    }

    #[test]
    fn a_lone_derive_reserves_too_and_counts_a_partial_byte() {
        let bytes = sized!(Bare {
            a: u4::new(0xA),
            b: 0xBCDE,
            run: vec![0x11; 9],
        });
        // 4 + 16 + 72 bits: twelve bytes, the last one half used.
        assert_eq!(bytes.len(), 12);
        assert_eq!(bytes.capacity(), 12);
    }

    /// Fields that are on the struct and not on the wire: the hint leaves them out.
    mod unwritten {
        use bnb::bin;
        use bnb::prelude::*;

        #[bin(big)]
        struct Derived {
            kind: u8,
            // Resolved on decode from `kind`; writes nothing.
            #[br(calc = u32::from(kind) * 2)]
            #[expect(dead_code, reason = "decode-side only")]
            doubled: u32,
            #[brw(count_prefix = u8)]
            body: Vec<u8>,
        }

        #[bin(big)]
        struct Peeked {
            kind: u8,
            // Read from the start again and the cursor restored; writes nothing.
            #[br(seek = 0usize.bytes(), restore_position)]
            again: u32,
            #[brw(count_prefix = u8)]
            body: Vec<u8>,
        }

        #[test]
        fn a_field_that_writes_nothing_is_not_in_the_size() {
            let derived = Derived {
                kind: 7,
                doubled: 14,
                body: vec![0x33; 20],
            };
            let bytes = derived.to_bytes().unwrap();
            assert_eq!(bytes.len(), 22);
            assert_eq!(bytes.capacity(), 22, "`br(calc)`");

            let peeked = Peeked {
                kind: 7,
                again: 0,
                body: vec![0x33; 20],
            };
            let bytes = peeked.to_bytes().unwrap();
            assert_eq!(bytes.len(), 22);
            assert_eq!(bytes.capacity(), 22, "`restore_position`");
        }
    }

    #[bin(big, write_only)]
    struct Outer {
        head: u16,
        // A variable-length message has no width for the hint: it counts as nothing.
        inner: Variable,
    }

    #[test]
    fn a_field_the_hint_cannot_size_only_makes_the_buffer_grow() {
        let bytes = sized!(Outer {
            head: 0x0102,
            inner: Variable {
                body: (0..40).collect(),
                tail: 0xDEAD_BEEF,
            },
        });
        assert_eq!(bytes.len(), 87);
        assert!(bytes.capacity() >= 87);
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
