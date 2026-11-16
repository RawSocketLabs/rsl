//! Doc-hidden helpers the generated codec calls: mapping, magic matching, byte arrays.

use alloc::borrow::Cow;
use alloc::string::ToString;
use core::fmt;

use crate::field::Bits;

use super::{BitDecode, BitEncode, BitError, CountPrefix, ErrorKind, Sink, Source};

/// The count or length prefix for `len` elements, or [`ErrorKind::CountOutOfRange`] at
/// bit offset `at` if `P` cannot hold it. Behind `count_prefix`, [`WireLen`](crate::WireLen)
/// and the length-prefixed string codec.
#[doc(hidden)]
#[inline]
pub fn prefix_for_count<P: CountPrefix>(len: usize, at: usize) -> Result<P, BitError> {
    P::try_from_count(len).map_err(|_| BitError::count_out_of_range(len, P::BITS, at))
}

/// A `#[br(assert(...))]` message: borrowed when it is a plain literal, formatted (once)
/// when it interpolates anything.
#[doc(hidden)]
#[must_use]
pub fn assert_message(args: fmt::Arguments<'_>) -> Cow<'static, str> {
    match args.as_str() {
        Some(literal) => Cow::Borrowed(literal),
        None => Cow::Owned(alloc::fmt::format(args)),
    }
}

/// [`ErrorKind::InvalidUtf8`] at bit offset `at`, where the caller's read of the string
/// (its length prefix, if it has one) began.
#[cold]
pub(crate) fn invalid_utf8(error: &alloc::string::FromUtf8Error, at: usize) -> BitError {
    BitError::new(
        ErrorKind::InvalidUtf8 {
            valid_up_to: error.utf8_error().valid_up_to(),
        },
        at,
    )
}

/// The bit width of a [`Bits`] value's type. Generated `BIT_LEN` consts and the
/// alignment guard call this to size a `magic` constant whose type they only have
/// as an expression (the value is taken by reference purely to infer `T`).
#[doc(hidden)]
#[must_use]
pub const fn bits_of<T: Bits>(_value: &T) -> u32 {
    T::BITS
}

/// Reads a `magic` constant and verifies it equals `expected`, compared as
/// type-erased bits (so `T` needs only [`Bits`] — no `Copy`/`PartialEq`, and `T`
/// is pinned by the argument so the generated call site needs no turbofish). On
/// mismatch: [`ErrorKind::BadMagic`] at the magic's offset.
#[doc(hidden)]
pub fn verify_magic<T: Bits, S: Source>(r: &mut S, expected: T) -> Result<(), BitError> {
    let at = r.bit_pos();
    let found: T = r.read()?;
    let (e, g) = (expected.into_bits(), found.into_bits());
    if e != g {
        return Err(BitError::bad_magic(e, g, at));
    }
    Ok(())
}

/// Reads a wire value `W` (inferred from `f`'s argument type) and maps it to the
/// field type `T` — backs `#[br(map = …)]`.
///
/// # Errors
/// Propagates the read [`BitError`].
#[doc(hidden)]
pub fn read_mapped<W, T, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: Bits,
    S: Source,
    F: FnOnce(W) -> T,
{
    let raw: W = r.read()?;
    Ok(f(raw))
}

/// Fallible variant — backs `#[br(try_map = …)]`. A conversion error becomes an
/// [`ErrorKind::Convert`] at the value's offset.
///
/// # Errors
/// The read [`BitError`], or the converter's failure as [`ErrorKind::Convert`].
#[doc(hidden)]
pub fn read_try_mapped<W, T, E, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: Bits,
    S: Source,
    E: fmt::Display,
    F: FnOnce(W) -> Result<T, E>,
{
    let at = r.bit_pos();
    let raw: W = r.read()?;
    f(raw).map_err(|e| BitError::convert(e.to_string(), at))
}

/// Maps the field `T` to its wire value `W` and writes it — backs `#[bw(map = …)]`.
///
/// # Errors
/// Propagates the write [`BitError`].
#[doc(hidden)]
pub fn write_mapped<W, T, K, F>(w: &mut K, value: &T, f: F) -> Result<(), BitError>
where
    W: Bits,
    K: Sink,
    F: FnOnce(&T) -> W,
{
    w.write(f(value))
}

/// Decodes a whole wire **message** `W` (a `BitDecode`, inferred from `f`'s argument) and
/// maps it to the logical type — backs struct-level `#[bin(map = …)]`. The message dual of
/// [`read_mapped`] (whose `W` is a single `Bits` field).
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_mapped_msg<W, T, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: BitDecode,
    S: Source,
    F: FnOnce(W) -> T,
{
    Ok(f(W::bit_decode(r)?))
}

/// Fallible message map — backs struct-level `#[bin(try_map = …)]`. A conversion error
/// becomes an [`ErrorKind::Convert`] at the message's start offset.
///
/// # Errors
/// The decode [`BitError`], or the converter's failure as [`ErrorKind::Convert`].
#[doc(hidden)]
pub fn decode_try_mapped_msg<W, T, E, S, F>(r: &mut S, f: F) -> Result<T, BitError>
where
    W: BitDecode,
    S: Source,
    E: fmt::Display,
    F: FnOnce(W) -> Result<T, E>,
{
    let at = r.bit_pos();
    let w = W::bit_decode(r)?;
    f(w).map_err(|e| BitError::convert(e.to_string(), at))
}

/// Maps the logical type to its wire **message** `W` (a `BitEncode`) and encodes it —
/// backs struct-level `#[bin(bw_map = …)]`. The message dual of [`write_mapped`].
///
/// # Errors
/// Propagates the encode [`BitError`].
#[doc(hidden)]
pub fn encode_mapped_msg<W, T, K, F>(w: &mut K, value: &T, f: F) -> Result<(), BitError>
where
    W: BitEncode,
    K: Sink,
    F: FnOnce(&T) -> W,
{
    f(value).bit_encode(w)
}

/// Reads a fixed `[u8; N]` byte array (`N * 8` bits) from the cursor. Backs a
/// `[u8; N]` payload field; `N` is inferred from the field type. Variable-length
/// payloads (`Vec` + `#[br(count = …)]`) take a separate push-based path that
/// grows by element, so an attacker-controlled count can't over-allocate.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn read_byte_array<const N: usize, S: Source>(r: &mut S) -> Result<[u8; N], BitError> {
    let mut arr = [0u8; N];
    r.read_into(&mut arr)?;
    Ok(arr)
}

/// Match one byte-string magic in declaration order, consuming it only on a full match.
/// A finite EOF rules out this candidate; incremental exhaustion with a matching prefix
/// requests more input before any later candidate or fallback can be selected.
/// No lookahead allocation is needed: a magic that is all in memory at a byte boundary is
/// compared where it lies, and any other is read a byte at a time.
///
/// # Errors
/// Propagates non-finite-EOF source errors, rewinding before returning.
// Inline, so the generated chain compares each constant magic in place when the whole of
// it is in memory at a byte boundary. Every read of the per-byte loop would then have
// succeeded, so its answer is this one; anything else takes that loop, out of line.
#[doc(hidden)]
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub fn match_magic<S: Source>(r: &mut S, magic: &[u8]) -> Result<bool, BitError> {
    let start = r.bit_pos();
    if let Some(matched) = r.aligned_run(magic.len()).map(|run| run == magic) {
        if !matched {
            r.seek_to_bit(start)?;
        }
        return Ok(matched);
    }
    match_magic_per_byte(r, magic, start)
}

/// [`match_magic`] one byte at a time from `start`, for a magic that is not all in memory
/// at a byte boundary: it stops at the first differing byte, and reports running out of
/// input as the source does.
#[inline(never)]
fn match_magic_per_byte<S: Source>(
    r: &mut S,
    magic: &[u8],
    start: usize,
) -> Result<bool, BitError> {
    for &expected in magic {
        match r.read_bits(8) {
            Ok(found) if found == u128::from(expected) => {}
            Ok(_) => {
                r.seek_to_bit(start)?;
                return Ok(false);
            }
            Err(error) => {
                r.seek_to_bit(start)?;
                return if matches!(error.kind, ErrorKind::UnexpectedEof { .. }) {
                    Ok(false)
                } else {
                    Err(error.in_field("magic"))
                };
            }
        }
    }
    Ok(true)
}

/// With the longest magic's worth of bytes in memory at a byte boundary: the index of the
/// first of `magics`, in declaration order, that the input starts with, consuming it, or
/// `magics.len()` and nothing consumed when none does. No read of a [`match_magic`] chain
/// can fail on such input, so this is that chain's answer, from one lend and constant
/// compares. Otherwise `None`, consuming nothing: the caller runs the chain.
///
/// # Errors
/// The source's seek, after a lend. No source in this crate lends a run it cannot seek
/// back over, so none returns it.
#[doc(hidden)]
#[inline]
pub fn peek_magic<S: Source>(r: &mut S, magics: &[&[u8]]) -> Result<Option<usize>, BitError> {
    let start = r.bit_pos();
    let longest = magics.iter().map(|magic| magic.len()).max().unwrap_or(0);
    let peeked = r.aligned_run(longest).map(|window| {
        magics
            .iter()
            .position(|magic| window.starts_with(magic))
            .map_or((magics.len(), 0), |hit| (hit, magics[hit].len()))
    });
    let Some((hit, len)) = peeked else {
        return Ok(None);
    };
    // The lend consumed the longest magic: leave the cursor after the winner's.
    r.seek_to_bit(start + len * 8)?;
    Ok(Some(hit))
}

/// Writes a fixed `[u8; N]` byte array. Backs a `[u8; N]` payload field.
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn write_byte_array<const N: usize, K: Sink>(arr: &[u8; N], w: &mut K) -> Result<(), BitError> {
    w.write_bytes(arr)
}

#[cfg(test)]
mod unit {
    use crate::bitstream::BitReader;
    use crate::bitstream::incremental::IncrementalReader;

    use super::*;

    /// Magic dispatch as the generated code runs it: the winner from one look at the longest
    /// magic when that is lent, else from the chain; `one` is how the chain tries a magic.
    /// Returns the winner's index (`magics.len()` for none) or the chain's error.
    fn dispatch<S: Source>(
        r: &mut S,
        magics: &[&[u8]],
        peek: bool,
        one: impl Fn(&mut S, &[u8]) -> Result<bool, BitError>,
    ) -> Result<usize, (ErrorKind, usize)> {
        let flat = |e: BitError| (e.kind, e.at);
        if peek {
            if let Some(hit) = peek_magic(r, magics).map_err(flat)? {
                return Ok(hit);
            }
        }
        for (index, magic) in magics.iter().enumerate() {
            if one(r, magic).map_err(flat)? {
                return Ok(index);
            }
        }
        Ok(magics.len())
    }

    #[test]
    fn magic_dispatch_from_a_lent_run_picks_what_the_byte_by_byte_chain_picks() {
        // A magic that is a prefix of a later one, one that is a prefix of an earlier one,
        // a shared first byte, and the empty magic last (it matches anything).
        let sets: [&[&[u8]]; 3] = [
            &[b"AB", b"ABC", b"CDE", b"C", b"TUVWXY"],
            &[b"TUVWXY", b"TU", b"Z"],
            &[b"OK", b"ERROR:", b""],
        ];
        for magics in sets {
            // Every magic whole, cut short at each length, and with each byte changed, alone
            // and with bytes after it, so the longest magic is sometimes there and sometimes
            // not.
            let mut inputs = vec![Vec::new(), b"??".to_vec()];
            for magic in magics {
                for cut in 0..=magic.len() {
                    for tail in [&b""[..], b"\x07", b"\x07\x08\x09\x0A\x0B\x0C"] {
                        inputs.push([&magic[..cut], tail].concat());
                    }
                }
                for flip in 0..magic.len() {
                    let mut near = [magic, &b"\x07\x08\x09\x0A\x0B\x0C"[..]].concat();
                    near[flip] ^= 0x20;
                    inputs.push(near);
                }
            }
            for input in &inputs {
                // From a byte boundary and from inside a byte, behind one lead byte.
                let wire = [&[0xA5][..], input].concat();
                for start in [8, 5] {
                    let context = format!("{magics:?} input {input:02X?} from bit {start}");
                    let slice = || {
                        let mut reader = BitReader::new(&wire);
                        reader.seek_to_bit(start).unwrap();
                        reader
                    };
                    let chain = |r: &mut BitReader<'_>, magic: &[u8]| {
                        let from = r.bit_pos();
                        match_magic_per_byte(r, magic, from)
                    };
                    let (mut fast, mut slow) = (slice(), slice());
                    let expected = (dispatch(&mut slow, magics, false, chain), slow.bit_pos());
                    let got = (
                        dispatch(&mut fast, magics, true, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}");
                    let mut fast = slice();
                    let got = (
                        dispatch(&mut fast, magics, false, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, chain only");

                    // The incremental reader waits where the slice reader rules a magic out.
                    let chain = |r: &mut IncrementalReader<'_>, magic: &[u8]| {
                        let from = r.bit_pos();
                        match_magic_per_byte(r, magic, from)
                    };
                    let (mut fast, mut slow) =
                        (IncrementalReader(slice()), IncrementalReader(slice()));
                    let expected = (dispatch(&mut slow, magics, false, chain), slow.bit_pos());
                    let got = (
                        dispatch(&mut fast, magics, true, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, incremental");
                    let mut fast = IncrementalReader(slice());
                    let got = (
                        dispatch(&mut fast, magics, false, match_magic),
                        fast.bit_pos(),
                    );
                    assert_eq!(got, expected, "{context}, incremental, chain only");
                }
            }
        }
    }

    #[test]
    fn a_peek_that_cannot_see_the_longest_magic_consumes_nothing() {
        let magics: &[&[u8]] = &[b"OK", b"ERROR:"];
        // Five bytes: one short of the longest magic, though the first magic is all there.
        let mut reader = BitReader::new(b"OK\x07\x08\x09");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), None);
        assert_eq!(reader.bit_pos(), 0);
        // Six: the first magic wins and only its two bytes are consumed.
        let mut reader = BitReader::new(b"OK\x07\x08\x09\x0A");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), Some(0));
        assert_eq!(reader.bit_pos(), 16);
        // No magic there: the count of magics, and nothing consumed.
        let mut reader = BitReader::new(b"NOPE!!");
        assert_eq!(peek_magic(&mut reader, magics).unwrap(), Some(2));
        assert_eq!(reader.bit_pos(), 0);
    }

    #[test]
    fn an_assert_message_is_borrowed_unless_it_interpolates() {
        assert!(matches!(
            assert_message(format_args!("too deep")),
            Cow::Borrowed("too deep")
        ));
        let depth = 9;
        let formatted = assert_message(format_args!("depth {depth}"));
        assert!(matches!(&formatted, Cow::Owned(text) if text == "depth 9"));
    }
}
