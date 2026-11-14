//! Generated code keeps to names the user cannot collide with by accident: its generic
//! parameters are `__BnbS`/`__BnbK` and its locals carry the `__bnb_` prefix, so a message
//! may use types named `S` and `K` (or `__S` and `__K`), fields named like the old generated
//! locals, and helpers named like the old `#[bitfield]` view locals.

mod macro_ {
    use bnb::prelude::*;
    use bnb::{bin, bitfield, u2, u4};

    // ── User types named like the old generated generic parameters. ──
    #[bin(big)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct S {
        a: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct K {
        b: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct UsesBoth {
        s: S,
        k: K,
        // A user expression naming the type, inside the generated function body.
        #[br(map = |raw: u8| S { a: raw }.a)]
        #[bw(map = |value: &u8| K { b: *value }.b)]
        mapped: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Either {
        #[bin(magic = 1u8)]
        One(S),
        #[bin(magic = 2u8)]
        Two { k: K },
    }

    #[bin(big, ctx(n: u8))]
    #[derive(Debug, PartialEq)]
    struct Counted {
        s: S,
        #[br(count = n)]
        items: Vec<K>,
    }

    #[test]
    fn user_types_named_s_and_k_are_not_captured() {
        let both = UsesBoth {
            s: S { a: 1 },
            k: K { b: 2 },
            mapped: 3,
        };
        assert_eq!(both.to_bytes().unwrap(), [1, 2, 3]);
        assert_eq!(UsesBoth::decode_exact(&[1, 2, 3]).unwrap(), both);

        assert_eq!(
            Either::decode_exact(&[1, 9]).unwrap(),
            Either::One(S { a: 9 })
        );
        assert_eq!(Either::Two { k: K { b: 9 } }.to_bytes().unwrap(), [2, 9]);

        let counted = Counted::decode_with_exact(&[7, 8, 9], CountedCtx { n: 2 }).unwrap();
        assert_eq!(counted.items, [K { b: 8 }, K { b: 9 }]);
        assert_eq!(counted.to_bytes().unwrap(), [7, 8, 9]);
    }

    // ── User types named like the generic parameters before they took the `__Bnb` prefix. ──
    #[bin(big)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    #[allow(non_camel_case_types)]
    struct __S {
        a: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    #[allow(non_camel_case_types)]
    struct __K {
        b: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct UsesDunderBoth {
        s: __S,
        #[br(count = 1)]
        ks: Vec<__K>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum DunderEither {
        #[bin(magic = 1u8)]
        One(__S, __K),
    }

    #[test]
    fn user_types_named_dunder_s_and_k_are_not_captured() {
        let both = UsesDunderBoth::decode_exact(&[1, 2]).unwrap();
        assert_eq!(
            (both.s, both.ks.as_slice()),
            (__S { a: 1 }, &[__K { b: 2 }][..])
        );
        assert_eq!(both.to_bytes().unwrap(), [1, 2]);

        let one = DunderEither::decode_exact(&[1, 8, 9]).unwrap();
        assert_eq!(one, DunderEither::One(__S { a: 8 }, __K { b: 9 }));
        assert_eq!(one.to_bytes().unwrap(), [1, 8, 9]);
    }

    // ── Helpers named like the old `#[bitfield]` view locals: one called from an inlined
    // closure body, one given as the `read` path. ──
    const fn __masked(bits: u2) -> u8 {
        bits.value() + 10
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "the signature a view's `read` path is called with"
    )]
    const fn __raw(bits: u2, _field: &Viewed) -> u8 {
        bits.value() + 20
    }

    #[bitfield(u8)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Viewed {
        #[view(
            bits = 2,
            read = |bits: u2, _field: &Self| __masked(bits),
            write = |value: u8| u2::new(value - 10)
        )]
        inlined: u8,
        #[view(bits = 2, raw = u2, read = __raw, write = |value: u8| u2::new(value - 20))]
        by_path: u8,
        rest: u4,
    }

    #[test]
    fn helpers_named_like_the_old_view_locals_are_not_captured() {
        let viewed = Viewed::new().with_inlined(11).with_by_path(22);
        assert_eq!((viewed.inlined(), viewed.by_path()), (11, 22));
    }

    // ── Fields named like the old generated locals, read by a later directive. ──
    #[bin(big, ctx(scale: u8))]
    #[derive(Debug, PartialEq)]
    struct Elem {
        #[br(map = move |raw: u8| raw.wrapping_mul(scale))]
        #[bw(map = |value: &u8| *value / scale)]
        value: u8,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct OldLocals {
        __n: u8,
        __v: u8,
        __start: u8,
        __e: u8,
        #[br(count = __n, ctx { scale = __v })]
        items: Vec<Elem>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct OldPeekLocals {
        __pos: u8,
        __seek: u8,
        #[br(seek = 1usize.bytes(), restore_position)]
        peeked: u8,
        #[br(count = __pos)]
        items: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum OldVariantLocals {
        #[bin(magic = 1u8)]
        Scaled {
            __e: u8,
            __m: u8,
            #[br(count = __m, ctx { scale = __e })]
            items: Vec<Elem>,
        },
        #[bin(magic = 2u8)]
        Named {
            __f: u8,
            #[br(count = __f)]
            #[try_str]
            name: Vec<u8>,
        },
    }

    #[test]
    fn fields_named_like_the_old_generated_locals_are_not_captured() {
        let bytes = [2, 3, 0, 0, 1, 2];
        let value = OldLocals::decode_exact(&bytes).unwrap();
        assert_eq!(value.items, [Elem { value: 3 }, Elem { value: 6 }]);
        assert_eq!(value.to_bytes().unwrap(), bytes);

        // `__pos` is the count (1), not the cursor offset the peek saved.
        let peek = OldPeekLocals::decode_exact(&[1, 0xAA, 0xBB]).unwrap();
        assert_eq!((peek.peeked, peek.items.as_slice()), (0xAA, &[0xBB][..]));

        let scaled = OldVariantLocals::decode_exact(&[1, 3, 1, 2]).unwrap();
        assert_eq!(
            scaled,
            OldVariantLocals::Scaled {
                __e: 3,
                __m: 1,
                items: vec![Elem { value: 6 }]
            }
        );
        assert_eq!(scaled.to_bytes().unwrap(), [1, 3, 1, 2]);

        let named = OldVariantLocals::decode_exact(&[2, 2, b'o', b'k']).unwrap();
        assert_eq!(format!("{named:?}"), r#"Named { __f: 2, name: "ok" }"#);
    }

    /// The size hint of a variable-length message finds a width probe by method name. A
    /// trait of the user's that gives every type a plainly named method must neither
    /// answer for the probe nor make it ambiguous.
    mod blanket_methods {
        use bnb::bin;

        trait BitsByValue: Sized {
            fn bits(self) -> usize {
                usize::MAX / 2
            }
        }
        impl<T> BitsByValue for T {}

        trait BitsByRef {
            fn bits(&self) -> usize {
                usize::MAX / 2
            }
        }
        impl<T> BitsByRef for T {}

        #[bin(big)]
        struct Inner {
            #[brw(count_prefix = u8)]
            run: Vec<u8>,
        }

        #[bin(big)]
        struct Outer {
            head: u16,
            #[brw(count_prefix = u8)]
            body: Vec<u8>,
            // An element with no fixed width: the probe's fallback tier.
            #[brw(count_prefix = u8)]
            inners: Vec<Inner>,
        }

        // A fixed-length message reads the cursor of the window it decodes from. A method
        // of the user's that takes any value would answer before the reader's own.
        trait BitPosByValue: Sized {
            fn bit_pos(self) -> usize {
                0
            }
        }
        impl<T> BitPosByValue for T {}

        #[bin(big)]
        #[derive(Debug, PartialEq)]
        struct Fixed {
            a: u16,
            b: u16,
        }

        #[test]
        fn a_users_blanket_bit_pos_method_does_not_place_the_cursor() {
            assert_eq!(
                Fixed::decode_exact(&[1, 2, 3, 4]),
                Ok(Fixed {
                    a: 0x0102,
                    b: 0x0304
                })
            );
            let (value, used) = Fixed::decode_prefix(&[1, 2, 3, 4, 5]).unwrap();
            assert_eq!((value.b, used), (0x0304, 4));
            // The trait is in scope and does answer for an ordinary value.
            assert_eq!(BitPosByValue::bit_pos(7u8), 0);
        }

        #[test]
        fn a_users_blanket_bits_method_does_not_size_the_output() {
            let message = Outer {
                head: 0x0102,
                body: vec![0x44; 20],
                inners: Vec::new(),
            };
            let bytes = message.to_bytes().unwrap();
            assert_eq!(bytes.len(), 24);
            assert_eq!(bytes.capacity(), 24);
            // Both traits are in scope and do answer for an ordinary value.
            assert_eq!(BitsByValue::bits(0u8), usize::MAX / 2);
            assert_eq!(BitsByRef::bits(&0u8), usize::MAX / 2);
        }
    }
}
