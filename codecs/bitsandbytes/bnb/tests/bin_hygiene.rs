//! Generated code keeps to names the user cannot collide with by accident: its generic
//! parameters are `__S`/`__K` and its locals carry the `__bnb_` prefix, so a message may use
//! types named `S` and `K` and fields named like the old generated locals.

mod macro_ {
    use bnb::bin;
    use bnb::prelude::*;

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
}
