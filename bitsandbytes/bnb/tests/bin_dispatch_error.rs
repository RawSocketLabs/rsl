//! Typed diagnostics must not change dispatch, cursor, or retry semantics.

mod macro_ {
    use bnb::{BitBuf, BitError, BitReader, DispatchKind, DispatchValue, ErrorKind, Source, bin};

    /// Assert typed error identity and capture without inspecting implementation fields.
    fn details<T: 'static>(error: &BitError, kind: DispatchKind, observed: Option<&DispatchValue>) {
        let detail = error.dispatch_error_for::<T>().expect("originating enum");
        assert_eq!(detail.dispatch_kind(), kind);
        assert_eq!(detail.observed(), observed);
        assert!(error.dispatch_error_for::<u8>().is_none());
        assert!(!error.is_incomplete());
    }

    #[test]
    fn integer_widths_and_layouts_capture_the_interpreted_value() {
        macro_rules! check {
            ($order:ident, $bits:ident, $magic:tt, $value:expr, $wire:expr) => {{
                #[bin(bytes = $order, bits = $bits)]
                #[derive(Debug)]
                enum Closed {
                    #[bin(magic = $magic)]
                    Known,
                }
                let bytes = $wire;
                let error = Closed::decode_exact(&bytes).unwrap_err();
                details::<Closed>(
                    &error,
                    DispatchKind::Magic,
                    Some(&DispatchValue::Integer($value)),
                );
                assert_eq!(error.at, bytes.len() * 8);
                assert_eq!(error.field, Some("magic"));
                assert_eq!(Closed::peek_variant(&bytes).unwrap_err(), error);
                assert_eq!(
                    error.dispatch_error_for::<Closed>().unwrap().enum_name(),
                    "Closed"
                );
            }};
        }
        macro_rules! widths {
            ($order:ident, $bits:ident, $bytes:ident) => {
                check!($order, $bits, 1u8, 0xab, 0xabu8.$bytes());
                check!($order, $bits, 1u16, 0xabcd, 0xabcdu16.$bytes());
                check!($order, $bits, 1u32, 0x1234_abcd, 0x1234_abcdu32.$bytes());
                check!(
                    $order,
                    $bits,
                    1u64,
                    0x1234_5678_abcd_ef01,
                    0x1234_5678_abcd_ef01u64.$bytes()
                );
                check!(
                    $order,
                    $bits,
                    1u128,
                    u128::MAX - 5,
                    (u128::MAX - 5).$bytes()
                );
            };
        }
        widths!(big, msb, to_be_bytes);
        widths!(little, msb, to_le_bytes);
        widths!(big, lsb, to_be_bytes);
        widths!(little, lsb, to_le_bytes);
    }

    #[test]
    fn fixed_bytes_capture_only_the_discriminator() {
        macro_rules! check {
            ($magic:tt, $input:expr, $width:expr) => {{
                #[bin(big)]
                #[derive(Debug)]
                enum Closed {
                    #[bin(magic = $magic)]
                    Known(u16),
                }
                let input = $input;
                let error = Closed::decode_exact(input).unwrap_err();
                details::<Closed>(
                    &error,
                    DispatchKind::Magic,
                    Some(&DispatchValue::Bytes(input[..$width].to_vec())),
                );
                assert_eq!(error.at, $width * 8);
                assert_eq!(Closed::peek_variant(input).unwrap_err(), error);
                for end in 0..$width {
                    let short = Closed::decode_exact(&input[..end]).unwrap_err();
                    assert!(matches!(short.kind, ErrorKind::UnexpectedEof { .. }));
                    assert!(short.dispatch_error_for::<Closed>().is_none());
                }
            }};
        }
        check!(b"A", b"Xpayload", 1);
        check!(b"DATA", b"BAD!payload", 4);
        check!(
            b"0123456789012345678901234567890123456789",
            b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXpayload",
            40
        );
    }

    // Existing context structs require Clone; diagnostics must add no further bounds.
    #[derive(Clone, PartialEq, Eq)]
    enum Selector {
        Known,
        Unknown,
    }

    #[bin(big, ctx(kind: Selector), tag = kind)]
    #[derive(Debug)]
    enum Tagged {
        #[bin(tag = Selector::Known)]
        Known,
    }

    #[bin(big, ctx(kind: Selector), tag = kind)]
    #[derive(Debug, PartialEq)]
    enum Hybrid {
        #[bin(tag = Selector::Known, magic = b"OK")]
        Known,
        #[bin(magic = 1u16)]
        Wire,
    }

    #[test]
    fn tags_are_not_captured_and_hybrids_capture_only_final_wire_magic() {
        let error = Tagged::decode_tagged(Selector::Unknown, &[]).unwrap_err();
        details::<Tagged>(&error, DispatchKind::Tag, None);
        assert_eq!((error.at, error.field), (0, Some("tag")));
        let error = Hybrid::decode_with_exact(
            &[0xab, 0xcd],
            HybridCtx {
                kind: Selector::Unknown,
            },
        )
        .unwrap_err();
        details::<Hybrid>(
            &error,
            DispatchKind::TagOrMagic,
            Some(&DispatchValue::Integer(0xabcd)),
        );
        assert_eq!((error.at, error.field), (16, Some("magic")));
        assert_eq!(
            Hybrid::decode_with_exact(
                b"OK",
                HybridCtx {
                    kind: Selector::Known
                }
            )
            .unwrap(),
            Hybrid::Known
        );
        let signature = Hybrid::decode_with_exact(
            &[0, 1],
            HybridCtx {
                kind: Selector::Known,
            },
        )
        .unwrap_err();
        assert!(matches!(signature.kind, ErrorKind::Convert { .. }));
        assert!(signature.dispatch_error_for::<Hybrid>().is_none());
    }

    #[bin(big)]
    #[derive(Debug)]
    enum Variable {
        #[bin(magic = b"LOGIN")]
        Login,
        #[bin(magic = b"BYE")]
        Bye,
    }

    #[test]
    fn variable_magic_none_is_terminal_not_incomplete() {
        let error = Variable::decode_exact(b"XXpayload").unwrap_err();
        details::<Variable>(&error, DispatchKind::Magic, None);
        assert_eq!((error.at, error.field), (0, Some("magic")));
        assert_eq!(Variable::peek_variant(b"XXpayload").unwrap_err(), error);
        let mut buffer = BitBuf::bounded(32);
        buffer.push(b"L").unwrap();
        assert!(buffer.try_pull::<Variable>().unwrap_err().is_incomplete());
        // Finite EOF rules out this partial magic, as it did before typed diagnostics.
        details::<Variable>(
            &buffer.pull_eof::<Variable>().unwrap_err(),
            DispatchKind::Magic,
            None,
        );
        buffer.push(b"X").unwrap();
        assert_eq!(buffer.try_pull::<Variable>().unwrap_err(), error);
        assert_eq!(buffer.read::<u16>().unwrap(), 0x4c58);
    }

    #[bin(big, magic = b"P")]
    #[derive(Debug)]
    enum Outer {
        #[bin(magic = 1u8)]
        Nested(#[brw(variable)] Closed),
    }

    #[bin(big)]
    #[derive(Debug)]
    enum Closed {
        #[bin(magic = 1u8)]
        Known(u16),
    }

    mod same_name {
        #[bnb::bin(big)]
        #[derive(Debug)]
        pub(super) enum Closed {
            #[bin(magic = 1u8)]
            Known,
        }
    }

    #[test]
    fn nested_origin_is_type_identity_not_name_or_outer_field() {
        let error = Outer::decode_exact(&[b'P', 1, 255]).unwrap_err();
        details::<Closed>(
            &error,
            DispatchKind::Magic,
            Some(&DispatchValue::Integer(255)),
        );
        assert!(error.dispatch_error_for::<Outer>().is_none());
        assert!(error.dispatch_error_for::<same_name::Closed>().is_none());
        let other = same_name::Closed::decode_exact(&[255]).unwrap_err();
        assert!(other.dispatch_error_for::<Closed>().is_none());
        assert_eq!(
            other
                .dispatch_error_for::<same_name::Closed>()
                .unwrap()
                .enum_name(),
            "Closed"
        );
        assert_eq!((error.at, error.field), (24, Some("magic")));
        assert_eq!(
            error.to_string(),
            "unrecognized Closed discriminant: 0xff at bit 24 (field `magic`)"
        );
    }

    #[test]
    fn prefixed_enum_miss_is_attributed_after_its_prefix() {
        let error = Outer::decode_exact(&[b'P', 2, 0]).unwrap_err();
        details::<Outer>(
            &error,
            DispatchKind::Magic,
            Some(&DispatchValue::Integer(2)),
        );
        assert_eq!((error.at, error.field), (16, Some("magic")));
        assert_eq!(Outer::peek_variant(&[b'P', 2, 0]).unwrap_err(), error);
    }

    #[test]
    fn selected_payload_prefix_and_explicit_variant_errors_are_not_dispatch_misses() {
        for error in [
            Outer::decode_exact(b"X").unwrap_err(),
            Outer::peek_variant(b"X").unwrap_err(),
            Closed::decode_as_known(&[255]).unwrap_err(),
        ] {
            assert!(matches!(error.kind, ErrorKind::Convert { .. }));
            assert!(error.dispatch_error_for::<Closed>().is_none());
            assert!(error.dispatch_error_for::<Outer>().is_none());
        }
        let payload = Outer::decode_exact(&[b'P', 1, 1, 0]).unwrap_err();
        assert!(matches!(payload.kind, ErrorKind::UnexpectedEof { .. }));
        assert!(payload.dispatch_error_for::<Closed>().is_none());
    }

    #[test]
    fn finite_and_incremental_attempts_keep_offsets_and_unread_input() {
        let input = [0xaa, 0xff, 0xde, 0xad];
        let mut source = BitReader::new(&input);
        assert_eq!(source.read::<u8>().unwrap(), 0xaa);
        let error = Closed::decode(&mut source).unwrap_err();
        assert_eq!((error.at, source.bit_pos()), (16, 16));
        assert_eq!(source.read::<u16>().unwrap(), 0xdead);
        let mut buffer = BitBuf::bounded(4);
        buffer.push(&input).unwrap();
        assert_eq!(buffer.read::<u8>().unwrap(), 0xaa);
        for _ in 0..3 {
            assert_eq!(buffer.try_pull::<Closed>().unwrap_err(), error);
            assert_eq!(buffer.bit_pos(), 8);
        }
        assert_eq!(buffer.read::<u8>().unwrap(), 0xff);
        assert_eq!(buffer.read::<u16>().unwrap(), 0xdead);
    }

    #[test]
    fn fixed_magic_shortfalls_stay_incomplete_until_dispatch_can_decide() {
        #[bin(big)]
        #[derive(Debug)]
        enum Wide {
            #[bin(magic = 1u32)]
            Known,
        }
        let mut empty = BitBuf::bounded(8);
        assert_eq!(
            empty.try_pull::<Wide>().unwrap_err().kind,
            ErrorKind::Incomplete { needed: None }
        );
        assert!(empty.pull_eof::<Wide>().unwrap().is_none());
        for length in 1..4 {
            let mut buffer = BitBuf::bounded(8);
            buffer.push(&[0xff; 4][..length]).unwrap();
            let error = buffer.try_pull::<Wide>().unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::Incomplete {
                    needed: Some(4 - length)
                }
            );
            assert!(error.dispatch_error_for::<Wide>().is_none());
            assert_eq!(buffer.bit_pos(), 0);
            assert!(matches!(
                buffer.pull_eof::<Wide>().unwrap_err().kind,
                ErrorKind::UnexpectedEof { .. }
            ));
            buffer.push(&[0xff; 4][length..]).unwrap();
            buffer.push(b"tail").unwrap();
            details::<Wide>(
                &buffer.try_pull::<Wide>().unwrap_err(),
                DispatchKind::Magic,
                Some(&DispatchValue::Integer(u128::from(u32::MAX))),
            );
            assert_eq!(buffer.read::<u32>().unwrap(), u32::MAX);
            assert_eq!(buffer.read::<u32>().unwrap(), 0x7461_696c);
        }
    }

    #[test]
    fn dispatch_misses_preserve_non_byte_aligned_offsets() {
        let input = [0x1f, 0xe0]; // three ignored bits, 0xff discriminator, five zero bits
        let mut reader = BitReader::new(&input);
        reader.seek_to_bit(3).unwrap();
        let error = Closed::decode(&mut reader).unwrap_err();
        assert_eq!((error.at, reader.bit_pos()), (11, 11));
        details::<Closed>(
            &error,
            DispatchKind::Magic,
            Some(&DispatchValue::Integer(255)),
        );
        let mut buffer = BitBuf::bounded(2);
        buffer.push(&input).unwrap();
        buffer.seek_to_bit(3).unwrap();
        assert_eq!(buffer.try_pull::<Closed>().unwrap_err(), error);
        assert_eq!(buffer.bit_pos(), 3);
        assert_eq!(buffer.read::<u8>().unwrap(), 255);
        assert_eq!(buffer.read::<bnb::u5>().unwrap(), bnb::u5::new(0));
    }
}
