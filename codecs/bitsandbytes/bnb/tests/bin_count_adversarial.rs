//! Adversarial `#[br(count = …)]` decoding — the untrusted-`count` boundary the
//! fuzzer exercises but doesn't *assert* on. A `count` is attacker-controlled, so the
//! decoder must:
//!   * never pre-allocate from it (the push-based guard in `bnb-macros` `field_read`),
//!   * error *gracefully* (`UnexpectedEof`, naming the field + offset) when it runs the
//!     cursor past end-of-input,
//!   * leave the under-count case to `decode_exact` (a `TrailingBytes` error), never a
//!     silent partial read.
//!
//! Each wire image is built by *encoding* a struct whose stored `count` field disagrees
//! with its payload length (encode writes the count verbatim, then the elements) — a
//! compact way to forge the hostile "claims N, supplies fewer" frame.

mod macro_ {

    use bnb::{ErrorKind, bin, u4, u12};

    // Leaf-element Vec: sub-byte `tag` (so the message isn't trivially byte-aligned), a
    // `u8` count, then a `u8` payload.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Msg {
        tag: u4,
        n: u8,
        #[br(count = n)]
        data: Vec<u8>,
    }

    #[test]
    fn count_far_past_input_is_a_graceful_eof() {
        // Wire says n = 255 but only 3 elements follow.
        let hostile = Msg {
            tag: u4::new(0x5),
            n: 255,
            data: vec![0xAA, 0xBB, 0xCC],
        };
        let bytes = hostile.to_bytes().unwrap();
        let err = Msg::decode_exact(&bytes).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::UnexpectedEof { .. }),
            "a count past end-of-input must be a graceful EOF, got {:?}",
            err.kind
        );
        assert_eq!(err.field, Some("data"), "the error names the Vec field");
    }

    #[test]
    fn count_off_by_one_past_input_errors_at_the_last_element() {
        // Exactly the present count round-trips...
        let ok = Msg {
            tag: u4::new(1),
            n: 3,
            data: vec![1, 2, 3],
        };
        assert_eq!(Msg::decode_exact(&ok.to_bytes().unwrap()).unwrap(), ok);

        // ...one more than present fails reading the final (absent) element.
        let one_too_many = Msg {
            tag: u4::new(1),
            n: 4,
            data: vec![1, 2, 3],
        };
        let err = Msg::decode_exact(&one_too_many.to_bytes().unwrap()).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
        assert_eq!(err.field, Some("data"));
    }

    #[test]
    fn under_count_leaves_trailing_bytes_rather_than_silently_truncating() {
        // Wire says n = 1 but 3 elements follow: decode reads one, `decode_exact` then
        // rejects the unconsumed remainder instead of accepting a partial parse.
        let under = Msg {
            tag: u4::new(7),
            n: 1,
            data: vec![1, 2, 3],
        };
        let err = Msg::decode_exact(&under.to_bytes().unwrap()).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::TrailingBytes { .. }),
            "under-count must surface as TrailingBytes, got {:?}",
            err.kind
        );
    }

    // A *wide* count type: the value can dwarf any real buffer. If the decoder
    // pre-allocated `Vec::with_capacity(n)` this test would try to reserve ~4 GiB and
    // abort; it passing is the proof that the push-based guard holds.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct WideCount {
        tag: u4,
        n: u32,
        #[br(count = n)]
        data: Vec<u8>,
    }

    #[test]
    fn enormous_count_does_not_preallocate() {
        let hostile = WideCount {
            tag: u4::new(0),
            n: u32::MAX, // ~4.29 billion
            data: vec![0xAA, 0xBB],
        };
        let bytes = hostile.to_bytes().unwrap();
        let err = WideCount::decode_exact(&bytes).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::UnexpectedEof { .. }),
            "a u32::MAX count on a 2-byte payload must EOF quickly, not over-allocate; got {:?}",
            err.kind
        );
    }

    // Nested-element Vec: each element is itself a `#[bin]` message, so the over-read
    // happens inside the element decode.
    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    struct Record {
        a: u4,
        b: u12, // 16 bits, fixed
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Table {
        flags: u4,
        count: u8,
        #[br(count = count)]
        #[nested]
        records: Vec<Record>,
    }

    #[test]
    fn nested_element_count_past_input_is_a_graceful_eof() {
        let hostile = Table {
            flags: u4::new(0xF),
            count: 200,
            records: vec![
                Record {
                    a: u4::new(1),
                    b: u12::new(2),
                },
                Record {
                    a: u4::new(3),
                    b: u12::new(4),
                },
            ],
        };
        let bytes = hostile.to_bytes().unwrap();
        let err = Table::decode_exact(&bytes).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::UnexpectedEof { .. }),
            "a nested over-count must EOF inside the element decode, got {:?}",
            err.kind
        );
        // Position-aware errors keep the *innermost* span: the EOF surfaces inside the
        // absent Record's `b` field, not the outer `records` Vec (cf. the
        // `innermost_field_wins_the_span` invariant in bitstream_errors.rs).
        assert_eq!(err.field, Some("b"));
    }
}
