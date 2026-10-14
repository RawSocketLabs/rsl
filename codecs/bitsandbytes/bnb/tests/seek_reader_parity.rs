//! `BufSeekReader` is observably `SeekReader`: over the same bytes and layout, any sequence
//! of seeks and reads (any width, any bit offset, past the end, after an error) returns the
//! same values, the same errors, and the same cursor. `SeekReader` is the reference — it
//! re-seeks absolutely before every read, so it cannot be fooled by stale position state.
//! Every successful value is also checked against the in-memory `BitReader`, which shares
//! neither reader's I/O path, so a bug common to both seek readers still fails.
#![cfg(feature = "std")]

mod property {

    use bnb::{
        BitError, BitOrder, BitReader, BufSeekReader, ByteOrder, Layout, SeekReader, Source,
    };
    use proptest::prelude::*;
    use std::io::Cursor;

    /// One step against both readers.
    #[derive(Clone, Debug)]
    enum Op {
        /// Seek to a bit offset, taken modulo the file's bit length plus a tail past the end.
        Seek(usize),
        /// A raw read of any width, including the rejected `> 128`.
        Bits(u32),
        /// Byte-order-applied reads, the container-format case.
        U16,
        U32,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            any::<usize>().prop_map(Op::Seek),
            (0u32..=130).prop_map(Op::Bits),
            Just(Op::U16),
            Just(Op::U32),
        ]
    }

    fn layout() -> impl Strategy<Value = Layout> {
        (any::<bool>(), any::<bool>()).prop_map(|(msb, big)| Layout {
            bit: if msb { BitOrder::Msb } else { BitOrder::Lsb },
            byte: if big {
                ByteOrder::Big
            } else {
                ByteOrder::Little
            },
        })
    }

    /// Deterministic file contents from a seed (lengths span several 8 KiB buffers).
    fn bytes(len: usize, mut seed: u64) -> Vec<u8> {
        (0..len)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                seed.to_be_bytes()[0]
            })
            .collect()
    }

    fn apply<S: Source>(src: &mut S, op: &Op, bit_len: usize) -> Result<u128, BitError> {
        match *op {
            Op::Seek(raw) => src.seek_to_bit(raw % (bit_len + 64)).map(|()| 0),
            Op::Bits(n) => src.read_bits(n),
            Op::U16 => src.read::<u16>().map(u128::from),
            Op::U32 => src.read::<u32>().map(u128::from),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn buf_seek_reader_matches_seek_reader(
            len in 0usize..20_000,
            seed in any::<u64>(),
            layout in layout(),
            ops in prop::collection::vec(op(), 1..64),
        ) {
            let data = bytes(len, seed);
            let data_ref = data.clone();
            let mut reference = SeekReader::with_layout(Cursor::new(data.clone()), layout);
            let mut candidate = BufSeekReader::with_layout(Cursor::new(data), layout);
            for (i, op) in ops.iter().enumerate() {
                let at = reference.bit_pos();
                let expected = apply(&mut reference, op, len * 8);
                // `BitReader` bounds-checks seeks, so a seek (or zero-width read) past the end
                // succeeds only on the seek readers; any bit-consuming success must agree.
                let consumes = !matches!(op, Op::Seek(_) | Op::Bits(0));
                if consumes && expected.is_ok() {
                    let mut oracle = BitReader::with_layout(&data_ref, layout);
                    oracle.seek_to_bit(at).expect("a successful read started in bounds");
                    let independent = apply(&mut oracle, op, len * 8);
                    prop_assert_eq!(&expected, &independent, "step {} ({:?}) disagrees with BitReader", i, op);
                }
                let actual = apply(&mut candidate, op, len * 8);
                prop_assert_eq!(&actual, &expected, "step {} ({:?}) diverged", i, op);
                prop_assert_eq!(candidate.bit_pos(), reference.bit_pos(), "cursor diverged at step {}", i);
            }
        }
    }
}
