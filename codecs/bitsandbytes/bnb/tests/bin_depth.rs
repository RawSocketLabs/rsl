//! Decode depth budget: a message that contains itself through a `Vec` nests one level per
//! non-empty `Vec`, and past `MAX_DECODE_DEPTH` levels decoding fails with
//! `ErrorKind::TooDeep` instead of overflowing the stack.

mod macro_ {

    use bnb::{BitBuf, BitError, ErrorKind, MAX_DECODE_DEPTH, Source, StreamBitReader, bin};

    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    struct Node {
        n: u8,
        #[br(count = n)]
        kids: Vec<Node>,
    }

    // The same shape through the context-taking element loop.
    #[bin(ctx(tag: u8))]
    #[derive(Debug, PartialEq, Eq)]
    struct CtxNode {
        n: u8,
        #[br(count = n, ctx { tag })]
        kids: Vec<CtxNode>,
    }

    // Indirect recursion, A -> B -> A, which the macro cannot see.
    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    struct A {
        n: u8,
        #[br(count = n)]
        b: Vec<B>,
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    struct B {
        n: u8,
        #[br(count = n)]
        a: Vec<A>,
    }

    // A recursive node that also carries a run of leaves, which the budget does not charge.
    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    struct Leafy {
        n: u8,
        #[br(count = n)]
        kids: Vec<Leafy>,
        m: u8,
        #[br(count = m)]
        words: Vec<u16>,
    }

    // Recursion through an enum variant's payload.
    #[bin]
    #[derive(Debug, PartialEq, Eq)]
    enum Tree {
        #[bin(magic = 0u8)]
        Leaf,
        #[bin(magic = 1u8)]
        Branch {
            n: u8,
            #[br(count = n)]
            kids: Vec<Tree>,
        },
    }

    /// A chain `levels` non-empty `Vec`s deep: `levels` ones, then the leaf's zero.
    fn chain(levels: usize) -> Vec<u8> {
        let mut wire = vec![1; levels];
        wire.push(0);
        wire
    }

    /// Runs `f` on a 2 MiB stack (Tokio's worker default), where an unbounded decode of
    /// hostile input aborts the process.
    fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn depth(mut node: &Node) -> usize {
        let mut levels = 0;
        while let Some(kid) = node.kids.first() {
            node = kid;
            levels += 1;
        }
        levels
    }

    #[test]
    fn the_limit_itself_decodes() {
        let levels = MAX_DECODE_DEPTH as usize;
        let node = on_small_stack(move || Node::decode_exact(&chain(levels)).unwrap());
        assert_eq!(depth(&node), levels);
        // Drop on the main stack: the decoded value is as deep as the input.
    }

    /// Every reader counts the levels for itself: the limit decodes and one past it is
    /// refused whichever of them the message comes through.
    #[test]
    fn the_budget_holds_on_every_reader() {
        use bnb::{BitDecode, BitReader, BufSeekReader, BufSource, SeekReader};
        use std::io::Cursor;

        type Outcome = (&'static str, Result<usize, ErrorKind>);

        fn through(wire: &[u8]) -> Vec<Outcome> {
            let outcome = |decoded: Result<Node, BitError>| {
                decoded.map(|node| depth(&node)).map_err(|error| error.kind)
            };
            let cursor = || Cursor::new(wire.to_vec());
            let mut buffer = BitBuf::new();
            buffer.push(wire).unwrap();
            let mut whole = BitReader::new(wire);
            let mut region = whole.limit_bytes(wire.len()).unwrap();
            vec![
                (
                    "StreamBitReader",
                    outcome(Node::decode(&mut StreamBitReader::new(wire))),
                ),
                (
                    "BufSource",
                    outcome(Node::decode(&mut BufSource::new(wire))),
                ),
                (
                    "SeekReader",
                    outcome(Node::decode(&mut SeekReader::new(cursor()))),
                ),
                (
                    "BufSeekReader",
                    outcome(Node::decode(&mut BufSeekReader::new(cursor()))),
                ),
                ("BitBuf", outcome(buffer.try_pull::<Node>())),
                ("LimitedSource", outcome(Node::bit_decode(&mut region))),
                #[cfg(feature = "bytes")]
                (
                    "BytesReader",
                    outcome(Node::decode(&mut bnb::BytesReader::new(
                        wire.to_vec().into(),
                    ))),
                ),
            ]
        }

        let limit = MAX_DECODE_DEPTH as usize;
        let (at, past) =
            on_small_stack(move || (through(&chain(limit)), through(&chain(limit + 1))));
        for (reader, outcome) in at {
            assert_eq!(outcome, Ok(limit), "{reader}");
        }
        for (reader, outcome) in past {
            assert_eq!(outcome, Err(ErrorKind::TooDeep), "{reader}");
        }
    }

    #[test]
    fn one_past_the_limit_is_too_deep() {
        let levels = MAX_DECODE_DEPTH as usize + 1;
        let err: BitError = on_small_stack(move || Node::decode_exact(&chain(levels)).unwrap_err());
        assert_eq!(err.kind, ErrorKind::TooDeep);
        // Raised entering the over-limit `Vec`, after its count byte.
        assert_eq!(err.at, levels * 8);
    }

    #[test]
    fn hostile_depth_errors_instead_of_aborting() {
        let err = on_small_stack(|| Node::decode_exact(&chain(1 << 20)).unwrap_err());
        assert_eq!(err.kind, ErrorKind::TooDeep);
    }

    #[test]
    fn width_does_not_count() {
        // One `Vec` of 255 leaves is one level, however many elements it holds.
        let mut wire = vec![255];
        wire.extend([0; 255]);
        assert_eq!(Node::decode_exact(&wire).unwrap().kids.len(), 255);
    }

    #[test]
    fn ctx_loop_is_bounded() {
        let ok = MAX_DECODE_DEPTH as usize;
        on_small_stack(move || {
            assert!(CtxNode::decode_with_exact(&chain(ok), CtxNodeCtx { tag: 0 }).is_ok());
            let err =
                CtxNode::decode_with_exact(&chain(ok + 1), CtxNodeCtx { tag: 0 }).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooDeep);
            let err =
                CtxNode::decode_with_exact(&chain(1 << 20), CtxNodeCtx { tag: 0 }).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooDeep);
        });
    }

    #[test]
    fn indirect_recursion_is_bounded() {
        on_small_stack(|| {
            let err = A::decode_exact(&chain(1 << 20)).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooDeep);
        });
    }

    #[test]
    fn a_failed_decode_leaves_a_reused_source_uncharged() {
        // Too deep, then a chain at the limit on the same source: it decodes only if the
        // failed attempt gave back every level it entered.
        let ok = MAX_DECODE_DEPTH as usize;
        on_small_stack(move || {
            let mut wire = chain(ok + 1);
            let second = wire.len() * 8;
            wire.extend(chain(ok));
            let mut src = BitBuf::new();
            src.push(&wire).unwrap();
            assert_eq!(Node::decode(&mut src).unwrap_err().kind, ErrorKind::TooDeep);
            src.seek_to_bit(second).unwrap();
            assert_eq!(depth(&Node::decode(&mut src).unwrap()), ok);
        });
    }

    #[test]
    fn incremental_attempts_do_not_share_a_count() {
        // Fed a byte at a time, the attempts that run out of input do not add up.
        let ok = MAX_DECODE_DEPTH as usize;
        on_small_stack(move || {
            let mut buf = BitBuf::new();
            let wire = chain(ok);
            let (last, head) = wire.split_last().unwrap();
            for byte in head {
                buf.push(&[*byte]).unwrap();
                assert_eq!(buf.pull::<Node>().unwrap(), None);
            }
            buf.push(&[*last]).unwrap();
            assert_eq!(depth(&buf.pull::<Node>().unwrap().unwrap()), ok);
        });
    }

    #[test]
    fn leaf_runs_are_not_charged_on_any_source() {
        // At the limit, the innermost node holds a non-empty `Vec<u16>`. A slice takes the
        // bulk path and a stream the element loop; both must agree that this is in budget.
        let levels = MAX_DECODE_DEPTH as usize;
        on_small_stack(move || {
            let mut wire = vec![1; levels]; // n: one kid, all the way down
            wire.extend([0, 1, 0xAB, 0xCD]); // innermost: no kids, one word
            wire.extend(core::iter::repeat_n(0, levels)); // every outer node: m = 0
            assert!(Leafy::decode_exact(&wire).is_ok());
            let mut stream = StreamBitReader::new(&wire[..]);
            assert!(Leafy::decode(&mut stream).is_ok());
        });
    }

    #[test]
    fn enum_payload_recursion_is_bounded() {
        on_small_stack(|| {
            // Each level: magic 1, one kid; the innermost is a leaf.
            let wire = |levels: usize| {
                let mut wire = [1, 1].repeat(levels);
                wire.push(0);
                wire
            };
            let ok = MAX_DECODE_DEPTH as usize;
            assert!(Tree::decode_exact(&wire(ok)).is_ok());
            let err = Tree::decode_exact(&wire(ok + 1)).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooDeep);
            let err = Tree::decode_exact(&wire(1 << 19)).unwrap_err();
            assert_eq!(err.kind, ErrorKind::TooDeep);
        });
    }
}
