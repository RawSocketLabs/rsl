//! Conjunctive context dispatch and context aliases.
mod macro_ {
    use bnb::{bin, u2, u6};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Namespace {
        Standard,
        Vendor,
    }

    type Opcode = u6;

    #[bin(ctx(fid: Namespace, code: Opcode))]
    #[derive(Debug, PartialEq, Eq)]
    enum Body {
        #[bin(tag(fid = Namespace::Standard, code = 0x00))]
        Standard(u8),
        #[bin(tag(code = 0x00, fid = Namespace::Vendor))]
        Vendor(u8),
        #[bin(tag(fid = Namespace::Standard, code = 0x3fu8))]
        Maximum(u8),
        #[catch_all]
        Unknown {
            raw: u8,
            fid: Namespace,
            code: Opcode,
        },
    }

    #[test]
    fn both_conditions_select_and_capture_without_consuming_bits() {
        for fid in [Namespace::Standard, Namespace::Vendor] {
            for code in 0..=0x3f {
                let code = u6::new(code);
                let body = Body::decode_with_exact(&[0xa5], BodyCtx { fid, code }).unwrap();
                match (fid, code.value(), &body) {
                    (Namespace::Standard, 0x00, Body::Standard(0xa5))
                    | (Namespace::Vendor, 0x00, Body::Vendor(0xa5))
                    | (Namespace::Standard, 0x3f, Body::Maximum(0xa5)) => {}
                    (
                        _,
                        _,
                        Body::Unknown {
                            raw: 0xa5,
                            fid: captured_fid,
                            code: captured_code,
                        },
                    ) => {
                        assert_ne!(code.value(), 0x00);
                        assert!(!(fid == Namespace::Standard && code.value() == 0x3f));
                        assert_eq!((*captured_fid, *captured_code), (fid, code));
                    }
                    other => panic!("wrong variant: {other:?}"),
                }
                assert_eq!((body.fid(), body.code()), (fid, code));
                assert_eq!(body.to_bytes().unwrap(), [0xa5]);
            }
        }
    }

    #[bin(ctx(code: u8))]
    #[derive(Debug, PartialEq, Eq)]
    struct Child {
        #[br(count = code)]
        bytes: Vec<u8>,
    }

    #[bin(ctx(length: u8))]
    #[derive(Debug, PartialEq, Eq)]
    struct Parent {
        code: u8,
        #[br(ctx { code = length })]
        from_context: Child,
        #[br(ctx { code })]
        from_field: Child,
    }

    #[test]
    fn alias_source_resolves_in_parent_scope() {
        let bytes = [0x02, 0x11, 0x22, 0x33];
        let parent = Parent::decode_with_exact(&bytes, ParentCtx { length: 1 }).unwrap();
        assert_eq!(parent.from_context.bytes, [0x11]);
        assert_eq!(parent.from_field.bytes, [0x22, 0x33]);
        assert_eq!(
            parent.to_bytes_with(ParentCtx { length: 1 }).unwrap(),
            bytes
        );
    }

    // Variant-arm aliases: one source is the enum's ctx param (passed by value), the other a
    // stored sibling (a match-bound reference). Forwarding the ctx param reads context on
    // encode, so this enum keeps `to_bytes_with`.
    #[bin(ctx(length: u8))]
    #[derive(Debug, PartialEq, Eq)]
    enum Framed {
        #[bin(magic = 0x01u8)]
        Pair {
            n: u8,
            #[br(ctx { code = length })]
            head: Child,
            #[br(ctx { code = n })]
            tail: Child,
        },
    }

    #[test]
    fn variant_alias_sources_resolve_from_ctx_and_siblings() {
        let bytes = [0x01, 0x02, 0x11, 0x22, 0x33];
        let value = Framed::decode_with_exact(&bytes, FramedCtx { length: 1 }).unwrap();
        let Framed::Pair { n, head, tail } = &value;
        assert_eq!(
            (*n, head.bytes.as_slice(), tail.bytes.as_slice()),
            (2, &[0x11][..], &[0x22, 0x33][..])
        );
        assert_eq!(value.to_bytes_with(FramedCtx { length: 1 }).unwrap(), bytes);
    }

    // A `temp` field is not bound by the arm's pattern, so its `calc` reading the ctx param
    // of the same name is a real context read: the enum keeps `to_bytes_with`.
    #[bin(ctx(code: u8))]
    #[derive(Debug, PartialEq, Eq)]
    enum Echo {
        #[bin(magic = 0x01u8)]
        Code {
            #[br(temp)]
            #[bw(calc = code)]
            code: u8,
        },
    }

    #[test]
    fn temp_calc_reading_ctx_keeps_encode_with() {
        let value = Echo::decode_with_exact(&[0x01, 0x09], EchoCtx { code: 9 }).unwrap();
        assert_eq!(value, Echo::Code {});
        assert_eq!(
            value.to_bytes_with(EchoCtx { code: 9 }).unwrap(),
            [0x01, 0x09]
        );
    }

    // A primitive behind a `type` alias takes a parenthesized, type-checked constant.
    type Code = u8;

    #[bin(ctx(code: Code))]
    #[derive(Debug, PartialEq, Eq)]
    enum AliasedBody {
        #[bin(tag(code = (7)))]
        Known(u8),
        #[catch_all]
        Unknown { code: Code, raw: u8 },
    }

    #[test]
    fn an_aliased_primitive_selector_takes_a_typed_constant() {
        let known = AliasedBody::decode_with_exact(&[0x55], AliasedBodyCtx { code: 7 }).unwrap();
        assert_eq!(known, AliasedBody::Known(0x55));
        let other = AliasedBody::decode_with_exact(&[0x55], AliasedBodyCtx { code: 8 }).unwrap();
        assert_eq!(other.code(), 8);
        assert_eq!(other.to_bytes().unwrap(), [0x55]);
    }

    #[bin(ctx(fid: u8, code: u6))]
    #[derive(Debug, PartialEq, Eq)]
    enum NumericBody {
        #[bin(tag(fid = 0x00, code = 0x07))]
        Known(u8),
        #[catch_all]
        Unknown { fid: u8, code: u6, raw: u8 },
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    struct Message {
        flags: u2,
        #[br(temp)]
        #[bw(calc = self.body.code())]
        opcode: u6,
        #[br(temp)]
        #[bw(calc = self.body.fid())]
        feature_set_id: u8,
        #[br(ctx { fid = feature_set_id, code = opcode })]
        body: NumericBody,
    }

    #[test]
    fn aliases_use_calculated_temporary_fields_when_encoding() {
        for bytes in [[0xc7, 0x00, 0xab], [0xc7, 0x10, 0xab], [0xff, 0xff, 0xab]] {
            let message = Message::decode_exact(&bytes).unwrap();
            assert_eq!(message.to_bytes().unwrap(), bytes);
        }
    }
    #[bin(ctx(fid: u8, code: u6, length: u8), magic = b"P")]
    #[derive(Debug, PartialEq, Eq)]
    enum WithFallback {
        #[bin(tag(fid = 0x00, code = 0x07))]
        Known {
            #[br(count = length)]
            bytes: Vec<u8>,
        },
        // These are payload fields, not captured context, in a typed fallback.
        Other {
            fid: u8,
            code: u6,
            flags: u2,
        },
    }

    #[test]
    fn extra_context_prefix_and_typed_fallback_keep_payload_semantics() {
        let ctx = || WithFallbackCtx {
            fid: 0x10,
            code: u6::new(0x07),
            length: 1,
        };
        let value = WithFallback::decode_with_exact(&[b'P', 0xab, 0xff], ctx()).unwrap();
        assert_eq!(
            value,
            WithFallback::Other {
                fid: 0xab,
                code: u6::new(0x3f),
                flags: u2::new(3)
            }
        );
        assert_eq!(value.to_bytes().unwrap(), [b'P', 0xab, 0xff]);
        let known = WithFallback::decode_with_exact(
            &[b'P', 0xab],
            WithFallbackCtx {
                fid: 0x00,
                code: u6::new(0x07),
                length: 1,
            },
        )
        .unwrap();
        assert_eq!(known, WithFallback::Known { bytes: vec![0xab] });
    }

    #[bin(ctx(code: u6), read_only)]
    enum Closed {
        #[bin(tag(code = 0x07))]
        Known(u8),
    }

    #[test]
    fn closed_miss_and_truncated_payload_are_distinct() {
        let miss = Closed::decode_with_exact(
            &[0xab],
            ClosedCtx {
                code: u6::new(0x08),
            },
        );
        assert!(matches!(miss, Err(ref e) if e.dispatch_error_for::<Closed>().is_some()));
        let short = Closed::decode_with_exact(
            &[],
            ClosedCtx {
                code: u6::new(0x07),
            },
        );
        assert!(matches!(short, Err(ref e) if e.dispatch_error_for::<Closed>().is_none()));
        assert!(matches!(
            Closed::decode_with_exact(
                &[0xab],
                ClosedCtx {
                    code: u6::new(0x07)
                }
            ),
            Ok(Closed::Known(0xab))
        ));
    }
    #[bin(ctx(code: u8))]
    struct DecodeOnlyParent {
        length: u8,
        #[br(ctx { code = length })]
        body: Child,
    }

    #[test]
    fn alias_target_does_not_require_irrelevant_encoding_context() {
        let value =
            DecodeOnlyParent::decode_with_exact(&[0x01, 0xab], DecodeOnlyParentCtx { code: 99 })
                .unwrap();
        assert_eq!(value.to_bytes().unwrap(), [0x01, 0xab]);
    }
    macro_rules! ctx_value {
        (($value:ident : $ty:ty)) => {
            $value as $ty
        };
    }

    #[bin(ctx(n: u8))]
    struct MacroContext {
        #[br(temp)]
        #[bw(calc = ctx_value!((n: u8)))]
        value: u8,
    }

    #[test]
    fn context_inside_opaque_macro_arguments_is_still_forwarded() {
        let value = MacroContext::decode_with_exact(&[0xab], MacroContextCtx { n: 0 }).unwrap();
        assert_eq!(
            value.to_bytes_with(MacroContextCtx { n: 0xcd }).unwrap(),
            [0xcd]
        );
    }
}
