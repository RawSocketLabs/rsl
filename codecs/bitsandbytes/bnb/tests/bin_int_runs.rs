//! Counted runs of wider integers in `#[bin]` messages: a context-free `Vec<u16>`,
//! `Vec<u32>`, `Vec<u64>` or `Vec<u128>` field decodes through `BitDecode::decode_vec` and
//! encodes through `BitEncode::encode_slice`, which convert a byte-aligned run in one pass.
//! These pin that generated path to an otherwise identical message whose element types keep
//! the per-element trait defaults: identical bytes and identical errors (kind, bit position,
//! field) in all four layouts, byte-aligned and not. The override is reached through the
//! impl, so a type alias takes the same path.

mod macro_ {
    use bnb::{BitDecode, BitEncode, BitError, FixedBitLen, Sink, Source, bin, u3};

    // An integer that keeps the per-element `decode_vec`/`encode_slice` defaults: the reference.
    macro_rules! each {
        ($name:ident, $int:ty) => {
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            struct $name($int);

            impl BitDecode for $name {
                fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                    r.read::<$int>().map($name)
                }
            }

            impl BitEncode for $name {
                fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                    w.write(self.0)
                }
            }

            impl FixedBitLen for $name {
                const BIT_LEN: u32 = <$int>::BITS;
            }
        };
    }
    each!(Each16, u16);
    each!(Each32, u32);
    each!(Each64, u64);
    each!(Each128, u128);

    type Word = u16;

    // A bulk message and its per-element reference, with an optional 3-bit lead field that
    // knocks every run off the byte boundary.
    macro_rules! pair {
        ($bulk:ident, $each:ident, [$($layout:tt)*] $(, $lead:ident)?) => {
            #[bin($($layout)*)]
            #[derive(Debug, PartialEq)]
            struct $bulk {
                $($lead: u3,)?
                #[brw(count_prefix = u8)]
                words: Vec<Word>,
                #[brw(count_prefix = u8)]
                dwords: Vec<u32>,
                #[brw(count_prefix = u8)]
                qwords: Vec<u64>,
                #[brw(count_prefix = u8)]
                owords: Vec<u128>,
            }

            #[bin($($layout)*)]
            #[derive(Debug, PartialEq)]
            struct $each {
                $($lead: u3,)?
                #[brw(count_prefix = u8)]
                words: Vec<Each16>,
                #[brw(count_prefix = u8)]
                dwords: Vec<Each32>,
                #[brw(count_prefix = u8)]
                qwords: Vec<Each64>,
                #[brw(count_prefix = u8)]
                owords: Vec<Each128>,
            }
        };
    }

    pair!(MsbBig, MsbBigEach, [big, bits = msb]);
    pair!(MsbBigShifted, MsbBigShiftedEach, [big, bits = msb], lead);
    pair!(MsbLittle, MsbLittleEach, [little, bits = msb]);
    pair!(
        MsbLittleShifted,
        MsbLittleShiftedEach,
        [little, bits = msb],
        lead
    );
    pair!(LsbBig, LsbBigEach, [big, bits = lsb]);
    pair!(LsbBigShifted, LsbBigShiftedEach, [big, bits = lsb], lead);
    pair!(LsbLittle, LsbLittleEach, [little, bits = lsb]);
    pair!(
        LsbLittleShifted,
        LsbLittleShiftedEach,
        [little, bits = lsb],
        lead
    );

    // `len` values with every byte distinct and nonzero in the first few.
    fn run(len: usize) -> Vec<u128> {
        let mut value = 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10_u128;
        (0..len)
            .map(|_| {
                value = value.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(7) ^ 0xA5;
                value
            })
            .collect()
    }

    // For every length set: identical encodings, identical decodes, and an identical error
    // for every truncation of the wire (each byte of each run and each prefix).
    macro_rules! agree {
        ($test:ident, $bulk:ident, $each:ident $(, $lead:ident)?) => {
            #[test]
            #[allow(clippy::cast_possible_truncation)] // Narrowing the sample values is the point.
            fn $test() {
                for lens in [[0, 0, 0, 0], [1, 0, 2, 0], [0, 3, 0, 1], [9, 7, 5, 3], [65, 33, 17, 9]] {
                    let [words, dwords, qwords, owords] = lens.map(run);
                    let bulk = $bulk {
                        $($lead: u3::new(0b101),)?
                        words: words.iter().map(|&v| v as u16).collect(),
                        dwords: dwords.iter().map(|&v| v as u32).collect(),
                        qwords: qwords.iter().map(|&v| v as u64).collect(),
                        owords: owords.clone(),
                    };
                    let reference = $each {
                        $($lead: u3::new(0b101),)?
                        words: words.iter().map(|&v| Each16(v as u16)).collect(),
                        dwords: dwords.iter().map(|&v| Each32(v as u32)).collect(),
                        qwords: qwords.iter().map(|&v| Each64(v as u64)).collect(),
                        owords: owords.iter().map(|&v| Each128(v)).collect(),
                    };
                    let wire = bulk.to_bytes().unwrap();
                    assert_eq!(wire, reference.to_bytes().unwrap(), "{lens:?}");
                    assert_eq!($bulk::decode_exact(&wire).unwrap(), bulk);
                    for end in 0..wire.len() {
                        let got = $bulk::decode_exact(&wire[..end]).unwrap_err();
                        let want = $each::decode_exact(&wire[..end]).unwrap_err();
                        assert_eq!(got, want, "{lens:?} truncated to {end}");
                    }
                }
            }
        };
    }

    agree!(msb_big_aligned_runs_match_per_element, MsbBig, MsbBigEach);
    agree!(
        msb_big_unaligned_runs_match_per_element,
        MsbBigShifted,
        MsbBigShiftedEach,
        lead
    );
    agree!(
        msb_little_aligned_runs_match_per_element,
        MsbLittle,
        MsbLittleEach
    );
    agree!(
        msb_little_unaligned_runs_match_per_element,
        MsbLittleShifted,
        MsbLittleShiftedEach,
        lead
    );
    agree!(lsb_big_aligned_runs_match_per_element, LsbBig, LsbBigEach);
    agree!(
        lsb_big_unaligned_runs_match_per_element,
        LsbBigShifted,
        LsbBigShiftedEach,
        lead
    );
    agree!(
        lsb_little_aligned_runs_match_per_element,
        LsbLittle,
        LsbLittleEach
    );
    agree!(
        lsb_little_unaligned_runs_match_per_element,
        LsbLittleShifted,
        LsbLittleShiftedEach,
        lead
    );

    // On a byte-aligned cursor the wire bytes of an integer follow the byte order alone:
    // the bit order only says how bits fill a byte, and whole bytes fill it either way.
    #[test]
    fn aligned_run_bytes_follow_the_byte_order_in_both_bit_orders() {
        let big = [1, 0x01, 0x02, 1, 0x03, 0x04, 0x05, 0x06, 0, 0];
        let little = [1, 0x02, 0x01, 1, 0x06, 0x05, 0x04, 0x03, 0, 0];
        macro_rules! bytes {
            ($message:ident) => {
                $message {
                    words: vec![0x0102],
                    dwords: vec![0x0304_0506],
                    qwords: vec![],
                    owords: vec![],
                }
                .to_bytes()
                .unwrap()
            };
        }
        assert_eq!(bytes!(MsbBig), big);
        assert_eq!(bytes!(LsbBig), big);
        assert_eq!(bytes!(MsbLittle), little);
        assert_eq!(bytes!(LsbLittle), little);
    }

    #[test]
    fn truncated_run_names_its_field_and_element_offset() {
        let wire = MsbBig {
            words: vec![1, 2],
            dwords: vec![3, 4, 5],
            qwords: vec![],
            owords: vec![],
        }
        .to_bytes()
        .unwrap();
        // prefix + two words + prefix + one dword and three bytes of the next: the element
        // that starts at byte 10 is the one that cannot be read.
        let error = MsbBig::decode_exact(&wire[..13]).unwrap_err();
        assert_eq!(error.field, Some("dwords"));
        assert_eq!(error.at, 10 * 8);
    }

    /// Forward-only streams lend no run and keep the per-element path; they must decode the
    /// same values as the bulk slice path.
    #[cfg(feature = "std")]
    #[test]
    fn streaming_sources_decode_runs_like_slices() {
        use bnb::StreamBitReader;
        use std::io::Cursor;

        let message = LsbBig {
            words: vec![0x0102, 0xFFFE],
            dwords: vec![0x0304_0506],
            qwords: vec![7, u64::MAX - 1],
            owords: vec![u128::MAX / 3],
        };
        let wire = message.to_bytes().unwrap();
        let mut stream = StreamBitReader::with_layout(Cursor::new(&wire), LsbBig::LAYOUT);
        assert_eq!(LsbBig::bit_decode(&mut stream).unwrap(), message);
    }
}
