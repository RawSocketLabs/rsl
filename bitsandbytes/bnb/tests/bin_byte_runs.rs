//! Byte runs in `#[bin]` messages: a context-free `Vec<u8>` field encodes through
//! `BitEncode::encode_slice` and decodes through `BitDecode::decode_vec`, which a byte-aligned
//! cursor copies in bulk. These pin that generated path to an otherwise identical message whose
//! element type keeps the per-element trait defaults: identical bytes and identical errors
//! (kind, bit position, field), byte-aligned and not, in both bit orders. The `u8` override is
//! reached through the impl, so a type alias for `u8` takes the same path.

mod macro_ {
    use bnb::{BitDecode, BitEncode, BitError, FixedBitLen, Sink, Source, bin, u3};

    /// A byte that keeps the per-element `decode_vec`/`encode_slice` defaults: the reference.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Each(u8);

    impl BitDecode for Each {
        fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
            r.read::<u8>().map(Each)
        }
    }

    impl BitEncode for Each {
        fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
            w.write(self.0)
        }
    }

    impl FixedBitLen for Each {
        const BIT_LEN: u32 = 8;
    }

    type Octet = u8;

    // A bulk message and its per-element reference, with an optional 3-bit lead field that
    // knocks both byte runs off the byte boundary.
    macro_rules! pair {
        ($bulk:ident, $each:ident, [$($layout:tt)*] $(, $lead:ident)?) => {
            #[bin($($layout)*)]
            #[derive(Debug, PartialEq)]
            struct $bulk {
                $($lead: u3,)?
                #[brw(count_prefix = u8)]
                user: Vec<u8>,
                #[brw(count_prefix = u8)]
                pass: Vec<Octet>,
            }

            #[bin($($layout)*)]
            #[derive(Debug, PartialEq)]
            struct $each {
                $($lead: u3,)?
                #[brw(count_prefix = u8)]
                user: Vec<Each>,
                #[brw(count_prefix = u8)]
                pass: Vec<Each>,
            }
        };
    }

    pair!(MsbAligned, MsbAlignedEach, [big, bits = msb]);
    pair!(MsbShifted, MsbShiftedEach, [big, bits = msb], lead);
    pair!(LsbAligned, LsbAlignedEach, [little, bits = lsb]);
    pair!(LsbShifted, LsbShiftedEach, [little, bits = lsb], lead);

    fn run(len: usize, seed: u8) -> Vec<u8> {
        let mut byte = seed;
        (0..len)
            .map(|_| {
                let b = byte;
                byte = byte.wrapping_add(29);
                b
            })
            .collect()
    }

    fn each(bytes: &[u8]) -> Vec<Each> {
        bytes.iter().copied().map(Each).collect()
    }

    // For every length pair: identical encodings, identical decodes, and an identical error
    // for every truncation of the wire (each byte of both runs and both prefixes).
    macro_rules! agree {
        ($test:ident, $bulk:ident, $each:ident $(, $lead:ident)?) => {
            #[test]
            fn $test() {
                for (user_len, pass_len) in [(0, 0), (1, 0), (0, 1), (7, 3), (255, 255)] {
                    let (user, pass) = (run(user_len, 0x11), run(pass_len, 0xF0));
                    let bulk = $bulk {
                        $($lead: u3::new(0b101),)?
                        user: user.clone(),
                        pass: pass.clone(),
                    };
                    let reference = $each {
                        $($lead: u3::new(0b101),)?
                        user: each(&user),
                        pass: each(&pass),
                    };
                    let wire = bulk.to_bytes().unwrap();
                    assert_eq!(wire, reference.to_bytes().unwrap(), "{user_len}/{pass_len}");
                    assert_eq!($bulk::decode_exact(&wire).unwrap(), bulk);
                    for end in 0..wire.len() {
                        let got = $bulk::decode_exact(&wire[..end]).unwrap_err();
                        let want = $each::decode_exact(&wire[..end]).unwrap_err();
                        assert_eq!(got, want, "{user_len}/{pass_len} truncated to {end}");
                    }
                }
            }
        };
    }

    agree!(
        msb_aligned_runs_match_per_element,
        MsbAligned,
        MsbAlignedEach
    );
    agree!(
        msb_unaligned_runs_match_per_element,
        MsbShifted,
        MsbShiftedEach,
        lead
    );
    agree!(
        lsb_aligned_runs_match_per_element,
        LsbAligned,
        LsbAlignedEach
    );
    agree!(
        lsb_unaligned_runs_match_per_element,
        LsbShifted,
        LsbShiftedEach,
        lead
    );

    #[test]
    fn truncated_run_names_its_field_and_offset() {
        let wire = MsbAligned {
            user: run(4, 0),
            pass: run(4, 0),
        }
        .to_bytes()
        .unwrap();
        // prefix + four user bytes + prefix + two of four password bytes: byte 8 is missing.
        let error = MsbAligned::decode_exact(&wire[..8]).unwrap_err();
        assert_eq!(error.field, Some("pass"));
        assert_eq!(error.at, 8 * 8);
    }

    /// An enum variant's `Vec<u8>` takes the same generated `encode_slice` path.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Frame {
        #[bin(magic = 0x01u8)]
        Data {
            #[brw(count_prefix = u8)]
            body: Vec<u8>,
        },
    }

    #[test]
    fn enum_variant_run_round_trips() {
        let frame = Frame::Data { body: run(5, 3) };
        let wire = frame.to_bytes().unwrap();
        assert_eq!(wire, [0x01, 5, 3, 32, 61, 90, 119]);
        assert_eq!(Frame::decode_exact(&wire).unwrap(), frame);
    }

    /// A fixed array decodes through `read_into` rather than `read_bytes`.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Fixed {
        lead: u3,
        tag: [u8; 4],
        #[brw(count_prefix = u8)]
        body: Vec<u8>,
    }

    /// Forward-only streams keep the per-byte trait defaults; they must decode the same
    /// values, and fail at the same position, as the bulk slice path.
    #[cfg(feature = "std")]
    #[test]
    fn streaming_sources_decode_runs_like_slices() {
        use bnb::StreamBitReader;
        use std::io::Cursor;

        fn stream<T: BitDecode + BitEncode>(wire: &[u8]) -> Result<T, BitError> {
            T::bit_decode(&mut StreamBitReader::with_layout(
                Cursor::new(wire),
                T::LAYOUT,
            ))
        }

        let aligned = MsbAligned {
            user: run(255, 1),
            pass: run(7, 2),
        };
        let wire = aligned.to_bytes().unwrap();
        assert_eq!(stream::<MsbAligned>(&wire).unwrap(), aligned);
        let fixed = Fixed {
            lead: u3::new(0b011),
            tag: *b"SOCK",
            body: run(9, 4),
        };
        let wire = fixed.to_bytes().unwrap();
        assert_eq!(stream::<Fixed>(&wire).unwrap(), fixed);
        for end in [1, 3, 6, 10] {
            let got = stream::<Fixed>(&wire[..end]).unwrap_err();
            let want = Fixed::decode_exact(&wire[..end]).unwrap_err();
            assert_eq!(
                (got.at, got.field),
                (want.at, want.field),
                "truncated to {end}"
            );
        }
    }
}
