//! A fixed-length message decoded from a window of its own length against the
//! same message decoded field by field: the value or the whole error, and where
//! the cursor is left, for every input.
use crate::bitstream::{IncrementalReader, WINDOW_LEAF, WINDOW_WHOLE, sealed, window_kind};
use bnb::{
    BitBuf, BitDecode, BitEnum, BitError, BitOrder, BitReader, ByteOrder, FixedBitLen, Layout,
    Source, bin, u2, u4, u12,
};

/// Any source, made to lend nothing, so a message decodes from it field by field.
struct NoLend<S>(S);

impl<S: Source> sealed::Sealed for NoLend<S> {}

impl<S: Source> Source for NoLend<S> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        self.0.read_bits(n)
    }
    fn __bnb_depth(&mut self) -> &mut u16 {
        self.0.__bnb_depth()
    }
    fn bit_pos(&self) -> usize {
        self.0.bit_pos()
    }
    fn byte_order(&self) -> ByteOrder {
        self.0.byte_order()
    }
    fn bit_order(&self) -> BitOrder {
        self.0.bit_order()
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.0.seek_to_bit(pos)
    }
}

#[derive(BitEnum, Debug, PartialEq, Eq, Clone, Copy)]
#[bit_enum(u2)]
enum Mode {
    Off,
    On,
    Auto,
    Held,
}

/// Twelve bits: the window is two bytes and the message stops inside the second.
#[derive(BitDecode, Debug, PartialEq)]
struct Odd {
    a: u4,
    b: u8,
}

/// A slice reader that lends, and records each lend's length and how many reads
/// it served itself.
struct Counting<'a> {
    inner: BitReader<'a>,
    lends: Vec<usize>,
    reads: usize,
}

impl sealed::Sealed for Counting<'_> {}

impl Source for Counting<'_> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        self.reads += 1;
        self.inner.read_bits(n)
    }
    fn __bnb_depth(&mut self) -> &mut u16 {
        self.inner.__bnb_depth()
    }
    fn bit_pos(&self) -> usize {
        self.inner.bit_pos()
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.inner.seek_to_bit(pos)
    }
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        self.lends.push(n);
        Source::aligned_run(&mut self.inner, n)
    }
}

/// The lends and reads a decode of `T` asks of its source, and where it stops.
fn asks<T: BitDecode>(bytes: &[u8]) -> (Vec<usize>, usize, usize) {
    let mut source = Counting {
        inner: BitReader::new(bytes),
        lends: Vec::new(),
        reads: 0,
    };
    T::bit_decode(&mut source).expect("decodes");
    (source.lends, source.reads, source.inner.bit_pos())
}

#[test]
fn a_message_that_suits_a_window_asks_its_source_for_one_lend() {
    let bytes = [
        0xC0, 0xDE, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0x6E, 0x77, 0xAB, 0xCD,
    ];
    // Its whole length in one lend, and not one read: nested messages and all.
    assert_eq!(asks::<Aligned>(&bytes), (vec![12], 0, 96));
    assert_eq!(asks::<Outer>(&bytes), (vec![18], 0, 144));
    // The window covers the byte the message ends in; the cursor stops at the bit.
    assert_eq!(asks::<Odd>(&bytes), (vec![2], 0, 12));
    // A message the rule leaves out reads field by field and lends nothing.
    assert_eq!(asks::<HoldsWide>(&bytes), (vec![], 4, 32));
}

/// Whole bytes on byte boundaries.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Aligned {
    kind: u16,
    seq: u32,
    digest: [u8; 5],
    last: u8,
}

/// Fields of at most eight bits, one of them an enum, with a guard that can fail.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Narrow {
    mode: Mode,
    #[br(assert(level.value() < 12, "level out of range"))]
    level: u4,
    spare: u2,
    tail: u8,
}

/// A magic, then two messages that suit a window themselves.
#[bin(big, magic = 0xC0DEu16)]
#[derive(Debug, PartialEq)]
struct Outer {
    head: Aligned,
    flags: Narrow,
    crc: u16,
}

/// A four-bit magic puts the sixteen-bit field after it inside a byte.
#[bin(big, magic = u4::new(0xA))]
#[derive(Debug, PartialEq)]
struct NibbleMagic {
    low: u4,
    word: u16,
}

/// Twelve bits that end inside a byte: not a window, and so neither is its parent.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Wide {
    a: u4,
    b: u12,
    c: u8,
}

#[bin(big)]
#[derive(Debug, PartialEq)]
struct HoldsWide {
    first: u8,
    wide: Wide,
}

/// Claims `CLAIM` bits and to read as a leaf, and reads sixteen: what a wrong
/// hand-written width looks like to a parent.
#[derive(Debug, PartialEq)]
struct Liar<const CLAIM: u32>(u16);

impl<const CLAIM: u32> BitDecode for Liar<CLAIM> {
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        r.read().map(Self)
    }
}

impl<const CLAIM: u32> FixedBitLen for Liar<CLAIM> {
    const BIT_LEN: u32 = CLAIM;
    const __BNB_WINDOW: u8 = WINDOW_LEAF;
}

#[derive(BitDecode, Debug, PartialEq)]
#[bit_stream(allow_byte_aligned)]
struct TooSmall {
    head: u8,
    body: Liar<8>,
    tail: u8,
}

#[derive(BitDecode, Debug, PartialEq)]
#[bit_stream(allow_byte_aligned)]
struct TooLarge {
    head: u8,
    body: Liar<24>,
    tail: u8,
}

#[test]
fn the_rule_admits_what_a_window_serves_and_nothing_else() {
    for (fields, suits) in [
        (&[][..], true),
        (
            &[(1, WINDOW_LEAF), (7, WINDOW_LEAF), (16, WINDOW_LEAF)][..],
            true,
        ),
        // Eight bits or fewer from anywhere; whole bytes only from a boundary.
        (
            &[(3, WINDOW_LEAF), (8, WINDOW_LEAF), (5, WINDOW_LEAF)][..],
            true,
        ),
        (&[(3, WINDOW_LEAF), (16, WINDOW_LEAF)][..], false),
        // Wider than a byte and not whole bytes: never.
        (&[(12, WINDOW_LEAF), (4, WINDOW_LEAF)][..], false),
        (&[(8, WINDOW_LEAF), (20, WINDOW_LEAF)][..], false),
        // A nested message: one that suits a window, on a boundary.
        (&[(8, WINDOW_LEAF), (96, WINDOW_WHOLE)][..], true),
        (&[(4, WINDOW_LEAF), (96, WINDOW_WHOLE)][..], false),
        (&[(8, WINDOW_LEAF), (96, 0)][..], false),
    ] {
        assert_eq!(window_kind(fields) == WINDOW_WHOLE, suits, "{fields:?}");
    }
    assert_eq!(<Aligned as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_eq!(<Narrow as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_eq!(<Outer as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_ne!(<Wide as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_ne!(<HoldsWide as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    // The magic is on the wire before the fields and moves them all along: four
    // bits of it and four of `low` put `word` on a boundary. Half a byte less of
    // either and it would not be.
    assert_eq!(<NibbleMagic as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
}

/// `T` from a source at bit `start`, as it comes and made to lend nothing: the
/// same value or the same whole error, and the same cursor.
fn assert_same_from<T, S>(make: impl Fn() -> S, start: usize, context: &str)
where
    T: BitDecode + PartialEq + core::fmt::Debug,
    S: Source,
{
    let (mut lends, mut never) = (make(), NoLend(make()));
    if lends.seek_to_bit(start).is_err() {
        return;
    }
    never.seek_to_bit(start).unwrap();
    assert_eq!(
        T::bit_decode(&mut lends),
        T::bit_decode(&mut never),
        "{context}"
    );
    assert_eq!(lends.bit_pos(), never.bit_pos(), "{context}");
}

/// The same through every reader that lends: the slice reader, the incremental
/// reader behind `decode_prefix` and the transports, a `BitBuf`, a `BytesReader`,
/// and a window over the slice reader that ends where the input does.
fn assert_same_either_way<T: BitDecode + PartialEq + core::fmt::Debug>(
    bytes: &[u8],
    start: usize,
    layout: Layout,
) {
    let context = format!("{} bytes from bit {start}, {layout:?}", bytes.len());
    let slice = || BitReader::with_layout(bytes, layout);
    assert_same_from::<T, _>(slice, start, &context);
    assert_same_from::<T, _>(|| IncrementalReader(slice()), start, &context);
    let buffered = || {
        let mut buffer = BitBuf::new().with_layout(layout);
        buffer.push(bytes).unwrap();
        buffer
    };
    assert_same_from::<T, _>(buffered, start, &context);
    #[cfg(feature = "bytes")]
    assert_same_from::<T, _>(
        || bnb::BytesReader::with_layout(bytes.to_vec().into(), layout),
        start,
        &context,
    );

    if start % 8 == 0 && start / 8 <= bytes.len() {
        let (mut lends, mut never) = (slice(), slice());
        lends.seek_to_bit(start).unwrap();
        never.seek_to_bit(start).unwrap();
        let mut lends = lends.limit_bytes(bytes.len() - start / 8).unwrap();
        let mut never = NoLend(never.limit_bytes(bytes.len() - start / 8).unwrap());
        assert_eq!(
            T::bit_decode(&mut lends),
            T::bit_decode(&mut never),
            "{context}"
        );
        assert_eq!(lends.bit_pos(), never.bit_pos(), "{context}, in a window");
    }
}

/// Every prefix of `wire` and `wire` with bytes after it, from byte and bit
/// offsets, in both bit orders and byte orders.
fn assert_same_on_every_cut<T: BitDecode + PartialEq + core::fmt::Debug>(wire: &[u8]) {
    let padded = [&[0x5A][..], wire, &[0xA5; 3]].concat();
    for bit in [BitOrder::Msb, BitOrder::Lsb] {
        for byte in [ByteOrder::Big, ByteOrder::Little] {
            let layout = Layout { bit, byte };
            for len in 0..=padded.len() {
                for start in [0, 3, 8, 12, 16] {
                    assert_same_either_way::<T>(&padded[..len], start, layout);
                }
            }
        }
    }
}

#[test]
fn a_window_decodes_what_field_by_field_decodes_on_every_input() {
    // Each message's own bytes, and the same with one byte changed at a time, so
    // the guard and the magic are met both passing and failing.
    let aligned = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let narrow = [0b0110_1110, 0x77];
    let outer = [&[0xC0, 0xDE][..], &aligned, &narrow, &[0xAB, 0xCD]].concat();
    let wide = [0x12, 0x34, 0x56, 0x78];
    for flip in 0..=outer.len() {
        let mut bytes = outer.clone();
        if let Some(byte) = bytes.get_mut(flip) {
            *byte ^= 0xFF;
        }
        assert_same_on_every_cut::<Outer>(&bytes);
    }
    for value in 0..=u8::MAX {
        assert_same_on_every_cut::<Narrow>(&[value, 0x77]);
    }
    // A guard that fails in a message nested inside the outer window.
    let failing = [
        &[0xC0, 0xDE][..],
        &aligned,
        &[0b0111_0110, 0x77],
        &[0xAB, 0xCD],
    ]
    .concat();
    assert!(Outer::decode_exact(&failing).is_err());
    assert_same_on_every_cut::<Outer>(&failing);
    assert_same_on_every_cut::<Aligned>(&aligned);
    assert_same_on_every_cut::<HoldsWide>(&wide);
    assert_same_on_every_cut::<Odd>(&wide);
}

#[test]
fn a_wrong_declared_width_only_sizes_the_window() {
    let wire = [0x11, 0x22, 0x33, 0x44, 0x55];
    // Too small a claim: the window is a byte short, the read fails in it, and
    // the message is decoded field by field. Too large: the window is lent only
    // when that many bytes are there, and the cursor still stops where the
    // fields did.
    assert_same_on_every_cut::<TooSmall>(&wire);
    assert_same_on_every_cut::<TooLarge>(&wire);
    let mut reader = BitReader::new(&wire);
    let value = TooLarge::bit_decode(&mut reader).unwrap();
    assert_eq!((value.head, value.body.0, value.tail), (0x11, 0x2233, 0x44));
    assert_eq!(reader.bit_pos(), 32);
}

/// Suits a window and declines its own: for a message that mostly arrives in pieces.
#[bin(big, no_window)]
#[derive(Debug, PartialEq)]
struct Piecewise {
    kind: u16,
    seq: u32,
    last: u8,
}

#[bin(big)]
#[derive(Debug, PartialEq)]
struct HoldsPiecewise {
    pre: u8,
    inner: Piecewise,
}

#[test]
fn a_message_that_declines_a_window_still_suits_its_parents() {
    let wire = [0xEE, 1, 2, 3, 4, 5, 6, 7];
    // On its own: every field read where it lies, and no lend asked for.
    assert_eq!(asks::<Piecewise>(&wire[1..]), (vec![], 3, 56));
    let piecewise = Piecewise::decode_exact(&wire[1..]).unwrap();
    assert_eq!(
        (piecewise.kind, piecewise.seq, piecewise.last),
        (0x0102, 0x0304_0506, 7)
    );
    // To a parent it is a message that suits a window, as it would be without the
    // option: the parent asks for its whole length in one lend.
    assert_eq!(<Piecewise as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_eq!(asks::<HoldsPiecewise>(&wire), (vec![8], 0, 64));
    let holds = HoldsPiecewise::decode_exact(&wire).unwrap();
    assert_eq!((holds.pre, holds.inner), (0xEE, piecewise));
}

/// A message with its own `Drop` and a field that is not `Copy`: it cannot be
/// taken apart, and must be built exactly once.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Session {
    key: Aligned,
    seq: u16,
}

thread_local! { static DROPS: core::cell::Cell<u32> = const { core::cell::Cell::new(0) }; }

impl Drop for Session {
    fn drop(&mut self) {
        DROPS.set(DROPS.get() + 1);
    }
}

#[test]
fn a_message_with_its_own_drop_is_built_once() {
    assert_eq!(<Session as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    let wire = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0xAB, 0xCD];
    let session = Session::decode_exact(&wire).unwrap();
    assert_eq!((session.key.kind, session.seq), (0x0102, 0xABCD));
    assert_eq!(DROPS.get(), 0, "nothing built and discarded on the way");
    drop(session);
    assert_eq!(DROPS.get(), 1);
}

/// A traced field reports its offset in the input, once: a message with one
/// takes no window, and neither does a message that holds it.
#[cfg(feature = "std")]
#[test]
fn a_traced_message_is_kept_out_of_every_window() {
    #[bin(big)]
    struct Traced {
        #[br(dbg)]
        a: u8,
        b: u8,
    }

    #[bin(big)]
    struct HoldsTraced {
        pre: u8,
        inner: Traced,
    }

    assert_ne!(<Traced as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    assert_ne!(<HoldsTraced as FixedBitLen>::__BNB_WINDOW, WINDOW_WHOLE);
    let value = HoldsTraced::decode_exact(&[1, 2, 3]).unwrap();
    assert_eq!((value.pre, value.inner.a, value.inner.b), (1, 2, 3));
    // Neither asks its source for a window: every field is read where it lies.
    assert_eq!(asks::<Traced>(&[2, 3]), (vec![], 2, 16));
    assert_eq!(asks::<HoldsTraced>(&[1, 2, 3]), (vec![], 3, 24));
}
