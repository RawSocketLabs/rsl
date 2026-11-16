//! The codec traits ([`BitDecode`], [`BitEncode`], [`FixedBitLen`]) and their leaf impls.

use alloc::vec::Vec;

use crate::field::{BitOrder, Bits, ByteOrder};

#[cfg(doc)]
use super::BitWriter;
#[cfg(feature = "std")]
use super::encode_to_writer_with;
use super::{BitError, BitReader, ErrorKind, Layout, MAX_DECODE_DEPTH, Sink, Source};

/// A message decoded from a bit stream — the recursion point a
/// `#[derive(BitDecode)]` struct implements (reading each field in declaration
/// order). Leaf fields are any [`Bits`] type; nested messages recurse. Fixed- or
/// variable-length; a fixed-length message *also* implements [`FixedBitLen`].
///
/// Most users reach for [`#[bin]`](macro@crate::bin) (which derives this plus
/// [`BitEncode`] and a builder); the bare derives are the codec on its own, for fields
/// that straddle byte boundaries.
///
/// # Examples
///
/// ```
/// use bnb::{BitDecode, BitEncode, u4, u12};
///
/// // A 4-bit tag + a 12-bit length, straddling the byte boundary.
/// #[derive(BitDecode, BitEncode, Debug, PartialEq)]
/// struct Frame { tag: u4, len: u12 }
///
/// let f = Frame::decode_exact(&[0xAB, 0xCD]).unwrap();
/// assert_eq!(f, Frame { tag: u4::new(0xA), len: u12::new(0xBCD) });
/// assert_eq!(f.to_bytes().unwrap(), [0xAB, 0xCD]); // round-trips
/// ```
pub trait BitDecode: Sized {
    /// Decodes `Self` from any [`Source`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the source's [`BitError`].
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError>;

    /// Decode `count` consecutive elements. Used by generated context-free counted fields.
    /// The default grows only after each successful, forward-moving element; `u8` uses
    /// [`Source::read_bytes`] so buffered byte payloads can check availability before allocating.
    /// `u16`, `u32`, `u64` and `u128` convert a byte-aligned run that is wholly in memory in
    /// one pass, allocating once those bytes are known to be there, and otherwise read per
    /// element like the default.
    /// An override must return **exactly** `count` elements in wire order, preserve element
    /// decoding, and advance for every nonempty result. It must not allocate from an
    /// untrusted count before proving the corresponding input is available. Wrappers
    /// using the default `Source::read_bytes` retain its per-byte allocation behavior.
    ///
    /// # Errors
    /// A source/element error, or [`ErrorKind::NoProgress`] for a zero-width element.
    /// Like [`bit_decode`](Self::bit_decode), this direct source operation is not transactional;
    /// use a `BitBuf` attempt for rollback of buffered input.
    ///
    /// The default counts one level of [`MAX_DECODE_DEPTH`] for a non-empty `Vec`, because a
    /// message can contain itself only this way; the crate's leaf types (integers, `bool`,
    /// `uN`, and what `#[bitfield]`, `BitEnum` and `#[bitflags]` generate) override it and are
    /// never charged.
    fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
        nested_vec(r, count, |r| decode_vec_each(r, count))
    }
}

/// The element-by-element loop behind [`BitDecode::decode_vec`]'s default (which runs it one
/// depth level down), what leaf types decode with, and what every bulk override falls back
/// to and is tested against: it grows only after each successful, forward-moving element.
#[doc(hidden)]
#[inline]
pub fn decode_vec_each<T: BitDecode, S: Source>(
    r: &mut S,
    count: usize,
) -> Result<Vec<T>, BitError> {
    let mut values = Vec::new();
    for _ in 0..count {
        let start = r.bit_pos();
        let value = T::bit_decode(r)?;
        if r.bit_pos() <= start {
            return Err(BitError::new(ErrorKind::NoProgress, start));
        }
        values.push(value);
    }
    Ok(values)
}

/// A slice reader over the next `N` bytes, when the source lends them (a byte-aligned
/// cursor, all of them in memory), in the source's layout and at its nesting depth; the
/// source's cursor moves past them. A fixed-length message decodes its fields from this
/// reader with one availability check for the whole message: its length is the constant
/// `N`, so each field's bounds check and alignment test fold.
///
/// `N` only sizes the window. The generated caller moves the source's cursor to where the
/// fields actually stopped, and on any failure inside the window puts it back and decodes
/// field by field, so every error a message reports is that path's.
#[doc(hidden)]
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub fn fixed_window<S: Source, const N: usize>(r: &mut S) -> Option<BitReader<'_>> {
    let (order, byte) = (r.bit_order(), r.byte_order());
    let depth = *r.__bnb_depth();
    let bytes = r.aligned_run(N)?.first_chunk::<N>()?;
    Some(BitReader {
        bytes,
        bit_pos: 0,
        order,
        byte,
        depth,
    })
}

/// Runs the loop decoding `count` elements one level deeper, failing with
/// [`ErrorKind::TooDeep`] past [`MAX_DECODE_DEPTH`]; an empty `Vec` enters nothing. The
/// previous depth is restored whatever `f` returns, so a source that outlives a failed
/// decode is not charged for it.
#[doc(hidden)]
#[inline]
pub fn nested_vec<S: Source, T>(
    r: &mut S,
    count: usize,
    f: impl FnOnce(&mut S) -> Result<Vec<T>, BitError>,
) -> Result<Vec<T>, BitError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let depth = *r.__bnb_depth();
    if u32::from(depth) >= MAX_DECODE_DEPTH {
        return Err(too_deep(r.bit_pos()));
    }
    *r.__bnb_depth() = depth + 1;
    let out = f(r);
    *r.__bnb_depth() = depth;
    out
}

#[cold]
#[inline(never)]
fn too_deep(at: usize) -> BitError {
    BitError::new(ErrorKind::TooDeep, at)
}

/// The element-by-element loop behind [`BitEncode::encode_slice`]'s default, and what every
/// bulk override falls back to and is tested against.
#[inline]
pub(super) fn encode_slice_each<T: BitEncode, K: Sink>(
    items: &[T],
    w: &mut K,
) -> Result<(), BitError> {
    for item in items {
        item.bit_encode(w)?;
    }
    Ok(())
}

/// [`BitDecode::decode_vec`] for an `N`-byte integer: a byte-aligned run that is all in
/// memory is converted in one pass, `be` or `le` by the source's byte order, into a `Vec`
/// allocated only once [`Source::aligned_run`] has shown the bytes are there. Whole aligned
/// bytes read the same in both bit orders, so the bit order does not enter. Any other run
/// (unaligned, short, or a count whose byte length overflows) is [`decode_vec_each`]'s.
#[inline]
fn decode_int_run<T: BitDecode, S: Source, const N: usize>(
    r: &mut S,
    count: usize,
    be: impl Fn([u8; N]) -> T,
    le: impl Fn([u8; N]) -> T,
) -> Result<Vec<T>, BitError> {
    let big = r.byte_order() == ByteOrder::Big;
    let Some(run) = count.checked_mul(N).and_then(|bytes| r.aligned_run(bytes)) else {
        return decode_vec_each(r, count);
    };
    let wire = run
        .chunks_exact(N)
        .map(|chunk| <[u8; N]>::try_from(chunk).expect("chunks_exact yields N bytes"));
    Ok(if big {
        wire.map(be).collect()
    } else {
        wire.map(le).collect()
    })
}

/// [`BitEncode::encode_slice`] for an `N`-byte integer, the dual of [`decode_int_run`]: on
/// a byte-aligned sink the run is written in one pass into the tail [`Sink::aligned_tail`]
/// lends, `be` or `le` by the sink's byte order. Any other run is [`encode_slice_each`]'s.
#[inline]
fn encode_int_run<T: BitEncode + Copy, K: Sink, const N: usize>(
    items: &[T],
    w: &mut K,
    be: impl Fn(T) -> [u8; N],
    le: impl Fn(T) -> [u8; N],
) -> Result<(), BitError> {
    let big = w.byte_order() == ByteOrder::Big;
    let Some(tail) = items
        .len()
        .checked_mul(N)
        .and_then(|bytes| w.aligned_tail(bytes))
    else {
        return encode_slice_each(items, w);
    };
    let wire = tail.chunks_exact_mut(N).zip(items);
    if big {
        wire.for_each(|(slot, &item)| slot.copy_from_slice(&be(item)));
    } else {
        wire.for_each(|(slot, &item)| slot.copy_from_slice(&le(item)));
    }
    Ok(())
}

/// A message whose encoded length is a **compile-time constant** — i.e. it has no
/// variable-length (`count`-driven `Vec`) field. The derive implements this only
/// for fixed messages; it sizes a fixed byte region when the message is embedded
/// as a field in another message (its contribution to the parent's width). A
/// `count`-bearing message implements [`BitDecode`]/[`BitEncode`] but **not** this.
/// `Bits` leaves also implement it (their `BIT_LEN` is `Bits::BITS`), so a field's
/// width is computed uniformly whether it's a leaf or a nested message.
pub trait FixedBitLen {
    /// Total encoded width of the message in bits — the sum of its fields' widths.
    const BIT_LEN: u32;

    /// How this type reads, for a parent deciding whether to decode from a window of its
    /// whole length ([`fixed_window`]): [`window_kind`]'s `LEAF` for a value read in one
    /// `read` of `BIT_LEN` bits, `WHOLE` for a generated message every field of which
    /// suits a window, and the default, unknown, for anything else (a hand-written
    /// impl), which keeps a parent decoding field by field.
    #[doc(hidden)]
    const __BNB_WINDOW: u8 = WINDOW_UNKNOWN;
}

/// [`FixedBitLen::__BNB_WINDOW`]: nothing is known about how the type reads.
const WINDOW_UNKNOWN: u8 = 0;
/// [`FixedBitLen::__BNB_WINDOW`]: one `read` of the type's `BIT_LEN` bits.
#[doc(hidden)]
pub const WINDOW_LEAF: u8 = 1;
/// [`FixedBitLen::__BNB_WINDOW`]: a message whose fields all suit a window.
#[doc(hidden)]
pub const WINDOW_WHOLE: u8 = 2;

/// Whether a fixed-length message suits a window of its own length, from its fields'
/// `(width in bits, kind)` in order: [`WINDOW_WHOLE`] if every field is a leaf of at
/// most eight bits, a leaf of whole bytes that starts on a byte boundary, or a nested
/// message that itself suits a window and starts on one; otherwise unknown.
///
/// The rule leaves out a wider field that starts or ends inside a byte. Such a read is
/// fastest with sixteen bytes behind it (DESIGN §22, row 3), and a window the message's
/// own length takes them away, so a message with one decodes field by field as before.
#[doc(hidden)]
#[must_use]
pub const fn window_kind(fields: &[(u32, u8)]) -> u8 {
    let (mut at, mut index) = (0u32, 0);
    while index < fields.len() {
        let (width, kind) = fields[index];
        let aligned = at % 8 == 0;
        let suits = match kind {
            WINDOW_LEAF => width <= 8 || (width % 8 == 0 && aligned),
            WINDOW_WHOLE => aligned,
            _ => false,
        };
        if !suits {
            return WINDOW_UNKNOWN;
        }
        at = at.wrapping_add(width);
        index += 1;
    }
    WINDOW_WHOLE
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
///
/// Encoding has two forms: the required [`bit_encode`](Self::bit_encode)
/// is **verbatim** (exactly what's stored), and [`canonical_bit_encode`](Self::canonical_bit_encode)
/// is **canonical** (`reserved` → spec value, `calc` → recomputed). The default canonical
/// impl just calls `bit_encode`, so the two are identical unless a `#[bin]` message has a
/// `reserved` or non-`temp` `calc` field — in which case the derive overrides it.
pub trait BitEncode {
    /// The message's bit/byte order, used to size a fresh [`BitWriter`] when
    /// encoding to a `Vec`/writer. The derive sets it from the struct's declared
    /// `bit_order`/`bytes`; a hand-written impl that only ever encodes into a
    /// caller-supplied [`Sink`] can leave the default.
    const LAYOUT: Layout = Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Big,
    };

    /// Encodes `self` **verbatim** into any [`Sink`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError>;

    /// Encodes `self`'s **canonical** form into any [`Sink`]: `reserved` fields as their
    /// spec value, `calc` fields recomputed. Defaults to [`bit_encode`](Self::bit_encode)
    /// (verbatim == canonical) for messages with no `reserved`/`calc` field.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn canonical_bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
        self.bit_encode(w)
    }

    /// Encode every element of `items` verbatim, in order — the encode dual of
    /// [`BitDecode::decode_vec`], used by generated context-free `Vec` fields. The default
    /// calls [`bit_encode`](Self::bit_encode) per element; `u8` uses [`Sink::write_bytes`],
    /// which a byte-aligned [`BitWriter`] appends in one copy, and `u16`, `u32`, `u64` and
    /// `u128` write a byte-aligned run into it in one pass. An override must write exactly
    /// the bits, and fail with exactly the error, of that per-element loop.
    ///
    /// # Errors
    /// The first element's or sink's [`BitError`].
    fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError>
    where
        Self: Sized,
    {
        encode_slice_each(items, w)
    }
}

// A `Bits` leaf (a `uN`, a `#[bitfield]`, a `BitEnum`/`#[bitflags]`) is *also* field-codable:
// it decodes by reading its `BITS` bits and encodes by writing them. This lets `#[bin]` treat
// **every** field uniformly through `bit_decode`/`bit_encode`, so it needs no `#[nested]` marker
// to choose between "read bits" and "recurse into a message". (The `Bits` packing role — the
// reason these types exist — is untouched; this only *adds* the stream-codec impls.) No blanket
// `impl<T: Bits>` is possible (it would collide with the per-message derives under coherence),
// so the leaves are covered concretely here and the macros emit one for each user `Bits` type.
macro_rules! bits_leaf_codec {
    ($($t:ident),* $(,)?) => {$(
        impl BitDecode for $t {
            #[inline]
            fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                r.read::<$t>()
            }
            bits_leaf_codec!(@decode_bytes $t);
        }
        impl BitEncode for $t {
            #[inline]
            fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                w.write(*self)
            }
            bits_leaf_codec!(@encode_bytes $t);
        }
        // A leaf's fixed width is its `Bits::BITS`, so `#[bin]` can size it the same way it
        // sizes a fixed nested message — uniformly via `FixedBitLen`.
        impl FixedBitLen for $t {
            const BIT_LEN: u32 = <$t as Bits>::BITS;
            const __BNB_WINDOW: u8 = WINDOW_LEAF;
        }
    )*};
    // A `u8` run is a byte run: the sources/sinks bulk-copy it when byte-aligned. Matching the
    // impl (not the field's spelling) reaches aliases and generic element types too.
    (@decode_bytes u8) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            r.read_bytes(count)
        }
    };
    // A leaf cannot contain itself, so its runs are not charged against the depth budget.
    (@decode_bytes bool) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            decode_vec_each(r, count)
        }
    };
    // A wider integer is whole bytes too: a byte-aligned run is converted in one pass.
    (@decode_bytes $int:ident) => {
        #[inline]
        fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            decode_int_run(r, count, <$int>::from_be_bytes, <$int>::from_le_bytes)
        }
    };
    (@encode_bytes u8) => {
        #[inline]
        fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError> {
            w.write_bytes(items)
        }
    };
    (@encode_bytes bool) => {};
    (@encode_bytes $int:ident) => {
        #[inline]
        fn encode_slice<K: Sink>(items: &[Self], w: &mut K) -> Result<(), BitError> {
            encode_int_run(items, w, <$int>::to_be_bytes, <$int>::to_le_bytes)
        }
    };
}
bits_leaf_codec!(u8, u16, u32, u64, u128, bool);

// `std::net` address types as `#[bin]` fields — the network-codec convenience the protocols
// dogfooding surfaced (IPv4 headers otherwise model addresses as a raw `u32`). An address
// serializes as its `to_bits`/`from_bits` integer, so it follows the struct's byte order like
// any other integer field: in a `#[bin(big)]` message that's the octets in network order
// (`192.168.1.1` → `C0 A8 01 01`), which is what every real protocol wants. `std` only, since
// the types are `std::net`.
macro_rules! ip_addr_codec {
    ($($t:ty => $int:ty, $bits:expr);* $(;)?) => {$(
        #[cfg(feature = "std")]
        impl BitDecode for $t {
            #[inline]
            fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                Ok(<$t>::from_bits(r.read::<$int>()?))
            }
            #[inline]
            fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
                decode_vec_each(r, count)
            }
        }
        #[cfg(feature = "std")]
        impl BitEncode for $t {
            #[inline]
            fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                w.write(self.to_bits())
            }
        }
        #[cfg(feature = "std")]
        impl FixedBitLen for $t {
            const BIT_LEN: u32 = $bits;
            const __BNB_WINDOW: u8 = WINDOW_LEAF;
        }
    )*};
}
ip_addr_codec!(
    std::net::Ipv4Addr => u32, 32;
    std::net::Ipv6Addr => u128, 128;
);

impl<T, const N: usize> BitDecode for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    #[inline]
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        r.read::<Self>()
    }

    #[inline]
    fn decode_vec<S: Source>(r: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
        decode_vec_each(r, count)
    }
}

impl<T, const N: usize> BitEncode for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    #[inline]
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
        w.write(*self)
    }
}

impl<T, const N: usize> FixedBitLen for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    const BIT_LEN: u32 = <Self as Bits>::BITS;
    const __BNB_WINDOW: u8 = WINDOW_LEAF;
}

/// `encode(writer)` for any [`BitEncode`] message — encodes to a `Vec` (using the type's
/// [`LAYOUT`](BitEncode::LAYOUT)) **verbatim** and writes it to a [`std::io::Write`] sink.
/// A blanket-implemented extension trait, so bring it into scope
/// (`use bnb::prelude::*` or `use bnb::EncodeExt`) to call `.encode(&mut w)`. Only
/// with the `std` feature; in `no_std` use the generated `to_bytes`/`to_canonical_bytes`, or
/// [`bit_encode`](BitEncode::bit_encode)/[`canonical_bit_encode`](BitEncode::canonical_bit_encode)
/// over a [`Sink`].
#[cfg(feature = "std")]
pub trait EncodeExt: BitEncode {
    /// Encodes `self` **verbatim** to any [`std::io::Write`] (socket, file, `Vec`) — exactly
    /// what's stored. For the canonical form, encode `self.to_canonical().encode(&mut w)`, or
    /// use the inherent `to_bytes` (verbatim) / `to_canonical_bytes` (canonical) instead.
    ///
    /// # Errors
    /// [`ErrorKind::Io`] on a write failure, else the encode error.
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<(), BitError>
    where
        Self: Sized,
    {
        encode_to_writer_with(w, Self::LAYOUT, |bw| self.bit_encode(bw))
    }
}

#[cfg(feature = "std")]
impl<T: BitEncode> EncodeExt for T {}

/// Polymorphic decode **with context** `A` — the companion to a `#[bin(ctx(...))]`
/// type's inherent `decode_with`, for hand-written generic combinators and
/// trait-object parsing (ctx Layer 2). Every [`BitDecode`] type is `DecodeWith<()>`
/// (blanket), and a ctx type is `DecodeWith<…Ctx>`, so one bound `T: DecodeWith<A>`
/// spans both context-free and context-taking messages. Inherent `Type::decode_with`
/// call sites are unaffected.
pub trait DecodeWith<A>: Sized {
    /// Decodes `Self` from a [`Source`] given `args`.
    ///
    /// # Errors
    /// Propagates the decode [`BitError`].
    fn decode_with<S: Source>(r: &mut S, args: A) -> Result<Self, BitError>;
}

/// The dual of [`DecodeWith`] — polymorphic encode with context `A`.
pub trait EncodeWith<A> {
    /// Encodes `self` into a [`Sink`] given `args`.
    ///
    /// # Errors
    /// Propagates the encode [`BitError`].
    fn encode_with<K: Sink>(&self, w: &mut K, args: A) -> Result<(), BitError>;
}

impl<T: BitDecode> DecodeWith<()> for T {
    fn decode_with<S: Source>(r: &mut S, _args: ()) -> Result<Self, BitError> {
        T::bit_decode(r)
    }
}

impl<T: BitEncode> EncodeWith<()> for T {
    fn encode_with<K: Sink>(&self, w: &mut K, _args: ()) -> Result<(), BitError> {
        self.bit_encode(w)
    }
}

#[cfg(test)]
mod unit {
    use crate::bitstream::BitWriter;

    use super::*;

    #[test]
    fn leaf_canonical_encode_defaults_to_verbatim() {
        let mut a = BitWriter::new();
        let mut b = BitWriter::new();
        BitEncode::bit_encode(&0xABCDu16, &mut a).unwrap();
        BitEncode::canonical_bit_encode(&0xABCDu16, &mut b).unwrap();
        assert_eq!(a.into_bytes(), b.into_bytes());
    }

    #[test]
    fn leaf_decode_with_and_encode_with_unit_args() {
        let mut r = BitReader::new(&[0xAB, 0xCD]);
        assert_eq!(
            <u16 as DecodeWith<()>>::decode_with(&mut r, ()).unwrap(),
            0xABCD
        );
        let mut w = BitWriter::new();
        EncodeWith::encode_with(&0xABCDu16, &mut w, ()).unwrap();
        assert_eq!(w.into_bytes(), [0xAB, 0xCD]);
    }
}
