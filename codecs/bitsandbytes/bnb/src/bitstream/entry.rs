//! Entry-point helpers — the logic behind the `#[derive]`-generated inherent
//! methods (`Type::decode`/`peek`/`decode_exact`/`encode`/`to_bytes`). Kept here
//! so the logic lives in one place rather than monomorphized inline per type;
//! doc-hidden because the public surface is the generated methods.

use alloc::vec::Vec;
#[cfg(feature = "net")]
use core::mem;

use crate::field::BitOrder;

use super::incremental::decode_attempt;
use super::layout::bits_in;
use super::{BitDecode, BitEncode, BitError, BitReader, BitWriter, DecodeWith, ErrorKind, Layout};

/// Decode every message from `bytes` into a `Vec`, with the message's own byte/bit order baked
/// in — bit-aware, so messages that don't end on byte boundaries reassemble correctly. Backs
/// `Type::decode_all`. Whole bytes must hold whole messages; a sub-byte tail in the final byte
/// that does not decode is padding, as in [`decode_exact`]. A tail that does decode (a message
/// narrower than a byte) is kept: padding and such a message cannot be told apart.
///
/// # Errors
/// The first decode [`BitError`] (e.g. a truncated trailing message), or
/// [`ErrorKind::NoProgress`] at a message's start when it decodes without ending past that
/// start (a zero-width message, or one whose `seek` leaves the cursor at or before it).
#[doc(hidden)]
pub fn decode_all<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<Vec<T>, BitError> {
    decode_iter(bytes, layout).collect()
}

/// Whether a failed decode that started at bit `at` only ran into the final byte's padding:
/// fewer than 8 bits were left, and the decode either ran out of them or they are all zero.
/// Any other failure on a nonzero tail is a real rejection and is reported.
fn is_padding(bytes: &[u8], at: usize, bit: BitOrder, error: &BitError) -> bool {
    let Some(&last) = bytes.last() else {
        return false;
    };
    if bits_in(bytes.len()) - at >= 8 {
        return false;
    }
    // The unread bits of the final byte: the low ones under MSB-first, the high ones under LSB.
    let unread = match bit {
        BitOrder::Msb => 0xFF >> (at % 8),
        BitOrder::Lsb => 0xFF << (at % 8),
    };
    matches!(error.kind, ErrorKind::UnexpectedEof { .. }) || last & unread == 0
}

/// A lazy iterator decoding successive `T` from `bytes` (layout baked in) until the buffer is
/// drained, ending after the first error if one occurs. Backs `Type::decode_iter`. A sub-byte
/// tail that does not decode ends it cleanly, as in [`decode_all`]. A message that decodes
/// without ending past its start is that final error, [`ErrorKind::NoProgress`], and is not
/// yielded.
#[doc(hidden)]
pub fn decode_iter<T: BitDecode>(
    bytes: &[u8],
    layout: Layout,
) -> impl Iterator<Item = Result<T, BitError>> + '_ {
    let mut r = BitReader::with_layout(bytes, layout);
    let mut stopped = false;
    core::iter::from_fn(move || {
        if stopped || r.remaining_bits() == 0 {
            return None;
        }
        let before = r.bit_pos();
        match T::bit_decode(&mut r) {
            // Zero-width, or a backward `seek`: without this the iterator never ends.
            Ok(_) if r.bit_pos() <= before => {
                stopped = true;
                Some(Err(BitError::new(ErrorKind::NoProgress, before)))
            }
            Ok(v) => Some(Ok(v)),
            Err(e) if is_padding(bytes, before, layout.bit, &e) => {
                stopped = true;
                None
            }
            Err(e) => {
                stopped = true;
                Some(Err(e))
            }
        }
    })
}

/// Decodes one message from `bytes` without consuming the caller's buffer
/// (tail-tolerant). Backs `Type::peek`. Not a prefix decoder: a short slice is a definitive
/// [`ErrorKind::UnexpectedEof`], indistinguishable from malformed input — see
/// [`decode_prefix`] for that.
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_peek<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    T::bit_decode(&mut BitReader::with_layout(bytes, layout))
}

/// `decode_peek` over a caller-supplied closure (no consumption requirement) — backs a
/// `#[bin]` enum's `peek_variant`, which runs only the dispatch decision over `bytes`.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn decode_peek_with<T, F>(bytes: &[u8], layout: Layout, f: F) -> Result<T, BitError>
where
    F: FnOnce(&mut BitReader) -> Result<T, BitError>,
{
    f(&mut BitReader::with_layout(bytes, layout))
}

/// `decode_exact` over a caller-supplied decode closure rather than the
/// [`BitDecode`] trait — backs the `ctx`-parameterized `Type::decode_with_exact`
/// (a `ctx` type takes a context argument, so it has no plain `bit_decode`).
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the closure's error.
#[doc(hidden)]
pub fn decode_exact_with<T, F>(bytes: &[u8], layout: Layout, f: F) -> Result<T, BitError>
where
    F: FnOnce(&mut BitReader) -> Result<T, BitError>,
{
    let mut r = BitReader::with_layout(bytes, layout);
    let v = f(&mut r)?;
    let consumed = r.bit_pos().div_ceil(8);
    if consumed < bytes.len() {
        return Err(BitError::new(
            ErrorKind::TrailingBytes {
                remaining: bytes.len() - consumed,
            },
            r.bit_pos(),
        ));
    }
    Ok(v)
}

/// `to_bytes` over a caller-supplied encode closure — backs the `ctx`-parameterized
/// `Type::to_bytes_with`.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_to_vec_with<F>(layout: Layout, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// [`encode_to_vec`] into a buffer the caller keeps: `out` is emptied, the message is
/// encoded into its allocation, and it is handed back holding the message (or, after an
/// error, whatever was written before it). A caller that reuses `out` allocates only when
/// a message outgrows it.
///
/// # Errors
/// Propagates the encoder's [`BitError`].
#[cfg(feature = "net")]
pub(crate) fn encode_into<T: BitEncode>(
    value: &T,
    layout: Layout,
    out: &mut Vec<u8>,
) -> Result<(), BitError> {
    let mut bytes = mem::take(out);
    bytes.clear();
    let mut w = BitWriter {
        bytes,
        ..BitWriter::with_layout(layout)
    };
    let result = value.bit_encode(&mut w);
    *out = w.into_bytes();
    result
}

/// [`encode_to_vec_with`] into a buffer allocated once, for a message of `bit_len` bits —
/// backs `Type::to_bytes`/`to_canonical_bytes` of a fixed-length message, whose
/// [`FixedBitLen::BIT_LEN`] the macro passes. The length only sizes the allocation: an
/// encoder that writes more still grows the buffer, and one that writes less returns less.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_fixed_to_vec<F>(layout: Layout, bit_len: u32, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    w.bytes.reserve_exact(bit_len.div_ceil(8) as usize);
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// [`encode_to_vec_with`] into a buffer reserved for `bits` bits: the macro's estimate for
/// a message with no [`FixedBitLen`] to size from (the widths of its fixed fields plus
/// the element widths of its `Vec`s; zero for anything it cannot see). Only a hint. A
/// low estimate grows the buffer from at least the eight bytes an unhinted one starts
/// with, a zero estimate reserves nothing, and a reservation that fails is skipped. The
/// estimate never exceeds what a successful encode of the message writes.
///
/// # Errors
/// Propagates the closure's [`BitError`].
#[doc(hidden)]
pub fn encode_hinted_to_vec<F>(layout: Layout, bits: usize, f: F) -> Result<Vec<u8>, BitError>
where
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut w = BitWriter::with_layout(layout);
    let bytes = bits.div_ceil(8);
    if bytes != 0 {
        // Never less than a growing `Vec<u8>` starts with, so an estimate that saw only
        // part of the message costs no allocation the unhinted buffer would not make.
        w.bytes.try_reserve_exact(bytes.max(8)).ok();
    }
    f(&mut w)?;
    Ok(w.into_bytes())
}

/// Decodes and requires every **whole byte** consumed; a sub-byte tail in the
/// final byte is treated as padding. Backs `Type::decode_exact`.
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the decode error.
#[doc(hidden)]
pub fn decode_exact<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    decode_exact_with(bytes, layout, |r| T::bit_decode(r))
}

/// Decodes one message from the front of `bytes` and returns it with the **bits** it consumed —
/// a stateless prefix decode that, unlike [`decode_peek`], tells a short slice from a bad one.
/// Backs `Type::decode_prefix` (`eof = false`) and `Type::decode_prefix_eof` (`eof = true`).
///
/// Each call replays the decode from bit 0 of `bytes`; nothing is retained between calls, and
/// positions in errors are relative to `bytes`. Trailing input is ignored.
///
/// # Errors
/// With `eof = false`, physical shortage is [`ErrorKind::Incomplete`]: append input and retry
/// (a positive `needed` is a lower bound in bytes). With `eof = true`, shortage is definitive
/// ([`ErrorKind::UnexpectedEof`], or [`ErrorKind::IncompleteAtEof`] when a custom codec asks for
/// more). Either way, codec errors and logical-region ([`LimitedSource`]) overruns are
/// definitive, and a message that consumes no bits is [`ErrorKind::NoProgress`].
#[doc(hidden)]
pub fn decode_prefix_with<T: DecodeWith<A>, A>(
    bytes: &[u8],
    layout: Layout,
    args: A,
    eof: bool,
) -> Result<(T, usize), BitError> {
    decode_attempt(bytes, 0, layout, args, eof)
}

/// [`decode_prefix_with`] for a context-free [`BitDecode`] message.
///
/// # Errors
/// As [`decode_prefix_with`].
#[doc(hidden)]
pub fn decode_prefix<T: BitDecode>(
    bytes: &[u8],
    layout: Layout,
    eof: bool,
) -> Result<(T, usize), BitError> {
    decode_prefix_with(bytes, layout, (), eof)
}

/// Encodes `value` to a `Vec<u8>`. Backs `Type::to_bytes`.
///
/// # Errors
/// Propagates the encode [`BitError`].
#[doc(hidden)]
pub fn encode_to_vec<T: BitEncode>(value: &T, layout: Layout) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_layout(layout);
    value.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// Encode to a [`std::io::Write`] over a caller-supplied encode closure — backs
/// [`EncodeExt::encode`] (the closure picks `bit_encode` vs `canonical_bit_encode`).
///
/// # Errors
/// [`ErrorKind::Io`] on a write failure, else the closure's error.
#[cfg(feature = "std")]
#[doc(hidden)]
pub fn encode_to_writer_with<W, F>(w: &mut W, layout: Layout, f: F) -> Result<(), BitError>
where
    W: std::io::Write,
    F: FnOnce(&mut BitWriter) -> Result<(), BitError>,
{
    let mut bw = BitWriter::with_layout(layout);
    f(&mut bw)?;
    let at = bw.bit_len();
    w.write_all(&bw.into_bytes())
        .map_err(|e| BitError::new(ErrorKind::Io(e.kind()), at))
}

#[cfg(test)]
mod unit {
    use crate::u4;

    use super::*;

    #[test]
    fn a_hinted_encode_reserves_its_bytes_once_and_shrugs_off_a_hint_it_cannot_meet() {
        let write = |n: usize| {
            move |w: &mut BitWriter| {
                (0..n).try_for_each(|_| w.write(0xA5u8))?;
                w.write(u4::new(0xF))
            }
        };
        // 84 bits: the partial eleventh byte is reserved with the first ten.
        let bytes = encode_hinted_to_vec(Layout::default(), 84, write(10)).unwrap();
        assert_eq!(bytes[8..], [0xA5, 0xA5, 0xF0]);
        assert_eq!(bytes.capacity(), 11);
        // A small estimate starts where an unhinted buffer does, and no estimate reserves
        // nothing at all.
        let bytes = encode_hinted_to_vec(Layout::default(), 20, write(2)).unwrap();
        assert_eq!(bytes.capacity(), 8);
        let empty = encode_hinted_to_vec(Layout::default(), 0, |_| Ok(())).unwrap();
        assert_eq!(empty.capacity(), 0);
        // A low hint grows; one no allocator can meet is skipped, not fatal.
        for hint in [0, 8, usize::MAX] {
            let bytes = encode_hinted_to_vec(Layout::default(), hint, write(40)).unwrap();
            assert_eq!(bytes.len(), 41, "hint {hint}");
        }
        // The closure's error is the result.
        let error = encode_hinted_to_vec(Layout::default(), 16, |w| w.write_bits(0, 129));
        assert_eq!(error.unwrap_err().kind, ErrorKind::TooWide { width: 129 });
    }

    #[test]
    fn decode_exact_with_accepts_input_the_closure_consumes_exactly() {
        let v = decode_exact_with(&[0xAB, 0xCD], Layout::default(), |r| r.read_bits(16));
        assert_eq!(v.unwrap(), 0xABCD);
    }

    #[test]
    fn decode_exact_with_rejects_a_trailing_byte() {
        let err = decode_exact_with(&[0xAB, 0xCD, 0xEE], Layout::default(), |r| r.read_bits(16))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TrailingBytes { remaining: 1 });
        assert_eq!(err.at, 16, "the error points at the first unconsumed bit");
    }

    #[test]
    fn decode_exact_with_accepts_the_unread_bits_of_a_final_partial_byte() {
        // Only *whole* trailing bytes are an error: 12 bits read out of 2 bytes is exact.
        let v = decode_exact_with(&[0xAB, 0xC0], Layout::default(), |r| r.read_bits(12));
        assert_eq!(v.unwrap(), 0xABC);
    }
}
