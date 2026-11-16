//! The read side of the cursor abstraction: [`Source`], [`SeekSource`] and the `Read` view.

use alloc::vec::Vec;

use crate::field::{BitOrder, Bits, ByteOrder};

use super::layout::value_byte_order;
use super::{BitError, BitReader, ErrorKind, Layout, LimitedSource, sealed};
#[cfg(all(doc, feature = "std"))]
use super::{BufSource, SeekReader};

/// A bit-level **input** the codec recurses over. Implemented by [`BitReader`]
/// (in-memory slice), `StreamBitReader` (forward `Read`), `BufSource` (a
/// retain-and-seek socket adapter), and `SeekReader`/`BufSeekReader` (`Read + Seek`); the codec is
/// generic over `Source`, so one decoder runs over any of them — see
/// [`guide::io`](crate::guide::io).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, Source, u4};
///
/// // A reader generic over any `Source`.
/// fn first_nibble<S: Source>(s: &mut S) -> u4 { s.read().unwrap() }
///
/// let mut r = BitReader::new(&[0xA5]);
/// assert_eq!(first_nibble(&mut r), u4::new(0xA));
/// ```
pub trait Source: sealed::Sealed {
    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the source's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// Propagates the reader's [`BitError`].
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError>;

    /// How many generated element loops are open on this source, for
    /// [`MAX_DECODE_DEPTH`]. Required, not defaulted, so every source (wrappers included)
    /// keeps its own count or forwards to the one it wraps.
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16;

    /// The current absolute bit offset (for position-aware errors).
    fn bit_pos(&self) -> usize;

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
    }

    /// The bit order this source reads in (default MSB-first). Paired with
    /// [`byte_order`](Source::byte_order) to decide whether a byte-multiple value needs its
    /// bytes swapped — each bit order has a *natural* byte layout (big-endian under MSB,
    /// little-endian under LSB), and only the opposite declaration swaps.
    fn bit_order(&self) -> BitOrder {
        BitOrder::Msb
    }

    /// Moves the cursor to absolute bit `pos`. The default — for a forward-only
    /// source — fails with [`ErrorKind::NotSeekable`]; seekable sources (the slice
    /// [`BitReader`]) override it. A [`SeekSource`] guarantees this works.
    ///
    /// # Errors
    /// [`ErrorKind::NotSeekable`] unless the source is seekable.
    fn seek_to_bit(&mut self, _pos: usize) -> Result<(), BitError> {
        Err(BitError::new(ErrorKind::NotSeekable, self.bit_pos()))
    }

    /// Reads one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits).
    #[inline]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        read_by_bits(self)
    }

    /// Reads `n` bytes into a fresh `Vec` (at any bit offset — the bytes need not be
    /// aligned). The bulk form of the per-byte `read::<u8>()` loop, for blob/payload
    /// reads in custom codecs and container formats.
    ///
    /// `n` is often attacker-controlled, so **nothing is pre-allocated from it**: bytes
    /// are pushed as they are read, bounded by the input — a hostile huge `n` against a
    /// short source is a fast [`UnexpectedEof`](ErrorKind::UnexpectedEof), not an
    /// allocation. The default is the correct-first per-byte loop; the slice-backed sources
    /// ([`BitReader`], `BitBuf`, `BytesReader`, and a [`LimitedSource`] window over one of
    /// them) copy a byte-aligned, fully available run in one step and otherwise run that
    /// same loop, so values, errors, and the cursor match. The readers over `std::io::Read`
    /// (`StreamBitReader`, `BufSource`) ask their reader for a byte-aligned run in steps: at
    /// most 4 KiB before any of it has arrived, then as much again as has, with the values,
    /// errors and cursor of the loop.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits).
    fn read_bytes(&mut self, n: usize) -> Result<alloc::vec::Vec<u8>, BitError> {
        read_bytes_per_byte(self, n)
    }

    /// Fills `buf` with bytes from the source — the no-alloc dual of
    /// [`read_bytes`](Source::read_bytes), for fixed scratch buffers and tight
    /// `no_std` paths. Bulk-copied under the same conditions as `read_bytes`.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits). What `buf` holds after an error is unspecified.
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        read_into_per_byte(self, buf)
    }

    /// Consumes `n` bytes and drops them (at any bit offset):
    /// [`read_bytes`](Source::read_bytes) without the `Vec`. For a body this decoder does
    /// not parse: an unknown record kind, padding a length field covers, a field of a newer
    /// version. It works on a source that cannot seek, and it fails if the bytes are not
    /// there, where a seek past the end of the data succeeds on `BufSource`, `SeekReader`,
    /// `BufSeekReader` and `BytesReader`.
    ///
    /// No `Vec` is made for the bytes, whatever `n` says: a run already in memory is passed
    /// over in one step, and any other is read through a small fixed buffer. Each source
    /// otherwise does what a read of the same bytes does. `BufSource` keeps what it skips,
    /// as it keeps all it reads so that it can seek back, and past its cap the skip fails
    /// with [`BufferFull`](ErrorKind::BufferFull); `SeekReader` reads a byte at a time
    /// (`BufSeekReader` does not).
    ///
    /// ```
    /// use bnb::{BitReader, Source};
    ///
    /// let mut r = BitReader::new(&[1, 2, 3, 4, 5]);
    /// r.skip_bytes(3).unwrap();
    /// assert_eq!(r.read::<u8>().unwrap(), 4);
    /// assert!(r.skip_bytes(2).is_err()); // one byte is left
    /// ```
    ///
    /// # Errors
    /// As [`read_into`](Source::read_into), for the first part of the run that is not
    /// there. In a decode attempt on partial input (`BitBuf::try_pull`, `decode_prefix`,
    /// the transports) the retry signal carries the whole shortfall, as `read_bytes`'s
    /// does, also from inside a [`limit_bytes`](Source::limit_bytes) region.
    fn skip_bytes(&mut self, n: usize) -> Result<(), BitError> {
        skip_bytes_in_steps(self, n)
    }

    /// Consumes and lends the next `n` bytes when the cursor is byte-aligned and all of them
    /// are already in memory; otherwise `None`, consuming nothing. The hook behind bulk
    /// counted runs of byte-multiple integers ([`BitDecode::decode_vec`]), behind
    /// byte-string magic dispatch and behind the window a fixed-length message decodes
    /// from: a caller that gets `None` reads element by element, or byte by byte, so every
    /// error is that path's. A source that lends must be able to
    /// [`seek_to_bit`](Source::seek_to_bit) back over what it lent, because a caller that
    /// decides against a run (a magic that did not match, a message that failed in its
    /// window) rewinds. The default lends nothing; the sources that bulk-copy
    /// [`read_bytes`](Source::read_bytes) override it.
    #[doc(hidden)]
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        let _ = n;
        None
    }

    /// Lends this source through a window of exactly `n` bytes.
    ///
    /// Reads and seeks through the returned [`LimitedSource`] cannot cross the declared region.
    /// Call [`LimitedSource::finish`] after decoding a nested value to require exact consumption.
    /// This is useful for length-delimited containers: the child decoder cannot accidentally
    /// consume bytes belonging to its parent.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotByteAligned`] when the source is between bytes, or
    /// [`ErrorKind::PositionOverflow`] when the region's end cannot be represented.
    fn limit_bytes(&mut self, n: usize) -> Result<LimitedSource<'_, Self>, BitError>
    where
        Self: Sized,
    {
        LimitedSource::bytes(self, n)
    }

    /// Borrows this source as a [`std::io::Read`] over its bytes — for handing the
    /// cursor to `std::io`-based code from a `#[br(parse_with = …)]` (e.g. a decoder, or
    /// a `Read`-based parser). Reads 8 bits per byte; see [`SourceReader`]. Only with
    /// the `std` feature.
    #[cfg(feature = "std")]
    fn as_read(&mut self) -> SourceReader<'_, Self>
    where
        Self: Sized,
    {
        SourceReader(self)
    }
}

/// [`Source::read`] for any source and width: the bits, then the byte order.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub(super) fn read_by_bits<S: Source + ?Sized, T: Bits>(s: &mut S) -> Result<T, BitError> {
    let raw = s.read_bits(T::BITS)?;
    Ok(T::from_bits(value_byte_order::<T>(
        raw,
        s.bit_order(),
        s.byte_order(),
    )))
}

// The per-byte loops behind the defaulted bulk methods. A bulk override takes its fast path
// only when the whole run is byte-aligned and available, and otherwise calls these, so every
// error, position, and partial-consumption effect is the per-byte loop's by construction.
pub(super) fn read_bytes_per_byte<S: Source + ?Sized>(
    s: &mut S,
    n: usize,
) -> Result<Vec<u8>, BitError> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(s.read::<u8>()?);
    }
    Ok(v)
}

/// [`Source::skip_bytes`] for any source: a run it lends is passed over whole, and any other
/// is read through a buffer on the stack, a step at a time, with that source's own bulk
/// `read_into`.
pub(super) fn skip_bytes_in_steps<S: Source + ?Sized>(s: &mut S, n: usize) -> Result<(), BitError> {
    if s.aligned_run(n).is_some() {
        return Ok(());
    }
    let mut dropped = [0; 256];
    let mut left = n;
    while left > 0 {
        let step = left.min(dropped.len());
        s.read_into(&mut dropped[..step])?;
        left -= step;
    }
    Ok(())
}

// Out of line: it is the fallback of every bulk `read_into`, and folded into a fixed
// `[u8; N]` read it is unrolled N times around the one copy that read wants.
#[inline(never)]
pub(super) fn read_into_per_byte<S: Source + ?Sized>(
    s: &mut S,
    buf: &mut [u8],
) -> Result<(), BitError> {
    for slot in buf.iter_mut() {
        *slot = s.read::<u8>()?;
    }
    Ok(())
}

// Runs a bulk read on a fresh slice reader at `*cursor`, then stores where it stopped — also
// after a failure, as the per-byte default leaves the cursor past every byte it read.
pub(super) fn read_via_slice<T>(
    bytes: &[u8],
    layout: Layout,
    cursor: &mut usize,
    read: impl FnOnce(&mut BitReader<'_>) -> Result<T, BitError>,
) -> Result<T, BitError> {
    let mut r = BitReader::with_layout(bytes, layout);
    r.seek_to_bit(*cursor)?;
    let result = read(&mut r);
    *cursor = r.bit_pos();
    result
}

/// [`BitReader::aligned_run`] for a source that keeps its own `cursor` into `bytes`: the
/// next `n` bytes, with the cursor moved past them, when it is byte-aligned and they are all
/// there. A cursor past the end (a `BytesReader` may seek there) has no run.
pub(super) fn aligned_run_at<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    n: usize,
) -> Option<&'a [u8]> {
    let mut r = BitReader::new(bytes);
    r.seek_to_bit(*cursor).ok()?;
    let run = r.aligned_run(n)?;
    *cursor = r.bit_pos();
    Some(run)
}

/// A [`std::io::Read`] view over a [`Source`], from [`Source::as_read`]. Each `read`
/// pulls 8 bits per byte through [`Source::read_bits`], so it works at any bit
/// alignment (you will normally be byte-aligned). A read failure surfaces as an
/// `io::Error` when no bytes were produced, or ends the read short once some were — the
/// `std::io` convention. This is the outbound dual of [`BufSource`]/[`SeekReader`] (which
/// adapt a `std::io::Read` *into* a `Source`).
#[cfg(feature = "std")]
pub struct SourceReader<'a, S: Source>(&'a mut S);

#[cfg(feature = "std")]
impl<S: Source> std::io::Read for SourceReader<'_, S> {
    #[allow(clippy::cast_possible_truncation)] // read_bits(8) yields at most one byte.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for (i, slot) in buf.iter_mut().enumerate() {
            match self.0.read_bits(8) {
                Ok(b) => *slot = b as u8,
                Err(e) if i == 0 => {
                    // Map the actual failure — only a genuine end-of-input is `UnexpectedEof`;
                    // a non-EOF error (e.g. `NotSeekable`, `Convert`) must not masquerade as one.
                    let kind = match e.kind {
                        ErrorKind::UnexpectedEof { .. } | ErrorKind::Incomplete { .. } => {
                            std::io::ErrorKind::UnexpectedEof
                        }
                        _ => std::io::ErrorKind::InvalidData,
                    };
                    return Err(std::io::Error::new(kind, e));
                }
                Err(_) => return Ok(i),
            }
        }
        Ok(buf.len())
    }
}

/// A [`Source`] that can seek (its [`seek_to_bit`](Source::seek_to_bit) is real, not
/// the failing default). A `#[bin]` message that uses `restore_position` bounds its
/// generated `decode` on this trait, so a forward-only stream is rejected at
/// compile time. Implemented by [`BitReader`], `BufSource`, `SeekReader`, and `BufSeekReader`
/// (and, with the `bytes` feature, `BytesReader`).
pub trait SeekSource: Source {}

impl SeekSource for BitReader<'_> {}

#[cfg(test)]
mod unit {
    use core::fmt;

    use crate::bitstream::fixtures::{Blank, Own, noise};
    use crate::bitstream::incremental::IncrementalReader;
    use crate::{u4, u12};

    use super::*;

    /// One `T` read at `pos` by each slice reader's `read`, against the same read taken
    /// bit by bit ([`read_by_bits`], the path every other source uses): the value or the
    /// error and its position, and where the cursor is left.
    fn assert_read_matches_the_bit_path<T: Bits + PartialEq + fmt::Debug>(
        buf: &[u8],
        layout: Layout,
        pos: usize,
    ) {
        type Outcome<T> = (Result<T, (ErrorKind, usize)>, usize);
        fn outcome<S: Source, T>(
            reader: &mut S,
            read: impl FnOnce(&mut S) -> Result<T, BitError>,
        ) -> Outcome<T> {
            let result = read(reader).map_err(|e| (e.kind, e.at));
            (result, reader.bit_pos())
        }
        let at = || {
            let mut reader = BitReader::with_layout(buf, layout);
            reader.bit_pos = pos;
            reader
        };
        let context = format!("{layout:?} len {} pos {pos} width {}", buf.len(), T::BITS);

        let expected: Outcome<T> = outcome(&mut at(), read_by_bits);
        assert_eq!(outcome(&mut at(), Source::read), expected, "{context}");
        assert_eq!(
            outcome(&mut at(), BitReader::read),
            expected,
            "{context}, inherent"
        );

        let expected: Outcome<T> = outcome(&mut IncrementalReader(at()), read_by_bits);
        let got = outcome(&mut IncrementalReader(at()), Source::read);
        assert_eq!(got, expected, "{context}, incremental");
    }

    #[test]
    fn read_takes_whole_bytes_exactly_as_the_bit_path_does() {
        use crate::{u24, u40, u72, u104};
        let mut state = 0xA076_1D64_78BD_642F;
        let noisy: Vec<u8> = (0..20)
            .map(|_| noise(&mut state).to_le_bytes()[0])
            .collect();
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };
                // Every length and every cursor in it, so each width meets its bytes all
                // there, one short and absent, from a byte boundary and from inside a byte.
                for len in 0..=noisy.len() {
                    let buf = &noisy[..len];
                    for pos in 0..=len * 8 {
                        macro_rules! each {
                            ($($value:ty),*) => {
                                $(assert_read_matches_the_bit_path::<$value>(buf, layout, pos);)*
                            };
                        }
                        each!(bool, u4, u12, u8, u16, u24, u32, u40, u64, u128);
                        each!(u72, u104, Own<true>, Own<false>, Blank<0>, Blank<136>);
                    }
                }
            }
        }
    }

    /// A forward-only `Source` that overrides only the two required methods, so calling
    /// the rest exercises the trait's default `byte_order`/`seek_to_bit`/`read`.
    struct TinySource<'a> {
        bytes: &'a [u8],
        pos: usize,
        depth: u16,
    }
    impl sealed::Sealed for TinySource<'_> {}

    impl Source for TinySource<'_> {
        fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
            let n = n as usize;
            let total = self.bytes.len() * 8;
            if self.pos + n > total {
                return Err(BitError::new(
                    ErrorKind::UnexpectedEof {
                        needed: n,
                        remaining: total - self.pos,
                    },
                    self.pos,
                ));
            }
            let mut acc = 0u128;
            for k in 0..n {
                let p = self.pos + k;
                acc = (acc << 1) | u128::from((self.bytes[p >> 3] >> (7 - (p & 7))) & 1);
            }
            self.pos += n;
            Ok(acc)
        }
        fn bit_pos(&self) -> usize {
            self.pos
        }
        fn __bnb_depth(&mut self) -> &mut u16 {
            &mut self.depth
        }
    }

    #[test]
    fn source_default_byte_order_is_big() {
        let s = TinySource {
            bytes: &[0],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.byte_order(), ByteOrder::Big);
    }

    #[test]
    fn source_default_seek_is_not_seekable() {
        let mut s = TinySource {
            bytes: &[0, 0],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.seek_to_bit(8).unwrap_err().kind, ErrorKind::NotSeekable);
    }

    #[test]
    fn source_default_read_dispatches_through_read_bits() {
        let mut s = TinySource {
            bytes: &[0xAB, 0xCD],
            pos: 0,
            depth: 0,
        };
        assert_eq!(s.read::<u8>().unwrap(), 0xAB);
        assert_eq!(s.read::<u8>().unwrap(), 0xCD);
    }

    #[test]
    fn source_default_byte_runs_read_per_byte() {
        let mut s = TinySource {
            bytes: &[0x0A, 0x0B, 0x0C, 0x0D, 0x0E],
            pos: 4,
            depth: 0,
        };
        // Unaligned: each byte straddles two input bytes.
        assert_eq!(s.read_bytes(2).unwrap(), [0xA0, 0xB0]);
        let mut buf = [0u8; 2];
        s.read_into(&mut buf).unwrap();
        assert_eq!(buf, [0xC0, 0xD0]);
        assert_eq!(s.bit_pos(), 36);
        let err = s.read_bytes(1).unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 4
            }
        ));
        assert!(s.read_into(&mut buf).is_err());
    }
}
