//! ctx Layer 2 (ROADMAP Phase 2): the polymorphic `DecodeWith<A>`/`EncodeWith<A>`
//! companion traits. Every `BitDecode` type is `DecodeWith<()>` (blanket); a
//! `#[bin(ctx(...))]` type is `DecodeWith<…Ctx>`. So one generic bound `T:
//! DecodeWith<A>` spans both context-free and context-taking messages — what a
//! hand-written combinator needs (the inherent `decode_with` call sites are
//! unchanged).

use bnb::{BitError, BitReader, DecodeWith, Source, bin, u4, u12};

#[bin]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Plain {
    a: u4,
    b: u12,
}

#[bin(ctx(n: u8))]
#[derive(Debug, PartialEq, Eq, Clone)]
struct WithCtx {
    flag: u4,
    #[br(count = n)]
    data: Vec<u8>,
}

// A generic combinator over *any* decodable-with-context message.
fn decode_one<T: DecodeWith<A>, A, S: Source>(r: &mut S, args: A) -> Result<T, BitError> {
    T::decode_with(r, args)
}

#[test]
fn one_bound_spans_context_free_and_context_taking() {
    // Context-free: A = ().
    let plain = Plain {
        a: u4::new(1),
        b: u12::new(0x234),
    };
    let bytes = plain.to_bytes().unwrap();
    let mut r = BitReader::new(&bytes);
    let got: Plain = decode_one(&mut r, ()).unwrap();
    assert_eq!(got, plain);

    // Context-taking: A = WithCtxCtx.
    let ctx = WithCtxCtx { n: 2 };
    let wc = WithCtx {
        flag: u4::new(0xF),
        data: vec![0xAA, 0xBB],
    };
    let bytes = wc.to_bytes_with(ctx.clone()).unwrap();
    let mut r = BitReader::new(&bytes);
    let got: WithCtx = decode_one(&mut r, ctx).unwrap();
    assert_eq!(got, wc);
}
