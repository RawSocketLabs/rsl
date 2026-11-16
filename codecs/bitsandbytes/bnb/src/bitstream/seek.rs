//! [`SeekReader`] and [`BufSeekReader`], sources over `Read + Seek`.

use crate::field::{BitOrder, ByteOrder};

use super::layout::extract_bits;
use super::{BitError, ErrorKind, Layout, SeekSource, Source, sealed};
#[cfg(doc)]
use super::{BufSource, MAX_DECODE_DEPTH};

/// A [`SeekSource`] over a seekable reader (`Read + Seek`, e.g. a `File`): it seeks
/// via [`std::io::Seek`] to the byte holding the bit cursor, **without buffering** —
/// the large-file / container-format case. For a *non*-seekable stream that still
/// needs to seek, use [`BufSource`].
///
/// It re-seeks to an absolute offset before **every** read, so it stays correct when the
/// cursor is shared (a `&File`, a clone of this reader, a handle others also move). The
/// price is one `seek` plus one `read` per value, and a `BufReader` inside `R` is emptied
/// by each seek. When the reader is exclusively owned, [`BufSeekReader`] reads the same
/// bytes with far fewer calls.
///
/// # Examples
///
/// ```
/// use bnb::{bin, SeekReader};
/// use std::io::Cursor;
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// let mut f = SeekReader::new(Cursor::new(vec![0x12u8, 0x34, 0x56, 0x78]));
/// assert_eq!(Word::decode(&mut f).unwrap(), Word { value: 0x1234_5678 });
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct SeekReader<R> {
    inner: R,
    bit_pos: usize,
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekReader<R> {
    /// Wraps `inner` at bit 0, MSB-first big-endian.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps `inner` at bit 0 with the given [`Layout`].
    #[must_use]
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner,
            bit_pos: 0,
            layout,
            depth: 0,
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> sealed::Sealed for SeekReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> Source for SeekReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let span = ByteSpan::new(self.bit_pos, n)?;
        self.inner
            .seek(std::io::SeekFrom::Start(span.start))
            .map_err(|e| span.io_error(&e))?;
        let acc = span.read(&mut self.inner, self.layout.bit)?;
        self.bit_pos = span.end;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.bit_pos = pos; // the actual `io::Seek` happens on the next read
        Ok(())
    }
}

/// The whole bytes an `n`-bit read at a bit offset touches — the shared read path of
/// [`SeekReader`] and [`BufSeekReader`], so both validate, fail, and extract identically.
#[cfg(feature = "std")]
struct ByteSpan {
    /// The read's starting bit offset (the position every error reports).
    at: usize,
    /// The read's width in bits (`<= 128`).
    width: u32,
    /// Absolute byte offset of the first touched byte.
    start: u64,
    /// Number of touched bytes (`<= 17`: 7 leading bits plus 128).
    len: usize,
    /// Bit offset of the read within the first byte.
    bit_off: usize,
    /// The bit offset just past the read.
    end: usize,
}

#[cfg(feature = "std")]
impl ByteSpan {
    /// The largest span: a 128-bit read starting 7 bits into a byte.
    const MAX_LEN: usize = (7 + 128usize).div_ceil(8);

    /// Validates an `n`-bit read at bit `at` before any I/O.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] for `n > 128`, [`ErrorKind::PositionOverflow`] when the end
    /// bit is unrepresentable.
    fn new(at: usize, n: u32) -> Result<Self, BitError> {
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n as usize }, at));
        }
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        let bit_off = at % 8;
        Ok(Self {
            at,
            width: n,
            start: (at / 8) as u64,
            len: (bit_off + n as usize).div_ceil(8),
            bit_off,
            end,
        })
    }

    /// Maps a positioning (seek) failure to [`ErrorKind::Io`] at the read's offset.
    fn io_error(&self, e: &std::io::Error) -> BitError {
        BitError::new(ErrorKind::Io(e.kind()), self.at)
    }

    /// Reads the span's bytes from `inner` (already positioned at [`start`](Self::start))
    /// into a stack buffer and extracts the value in `order`.
    ///
    /// # Errors
    /// End of input is [`ErrorKind::UnexpectedEof`]; any other failure is
    /// [`ErrorKind::Io`].
    fn read(&self, inner: &mut impl std::io::Read, order: BitOrder) -> Result<u128, BitError> {
        let mut buf = [0u8; Self::MAX_LEN];
        let bytes = &mut buf[..self.len];
        inner.read_exact(bytes).map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                ErrorKind::UnexpectedEof {
                    needed: self.width as usize,
                    remaining: 0,
                }
            } else {
                ErrorKind::Io(e.kind())
            };
            BitError::new(kind, self.at)
        })?;
        Ok(extract_bits(
            bytes,
            self.bit_off,
            self.width as usize,
            order,
        ))
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekSource for SeekReader<R> {}

/// A [`SeekSource`] that **owns** a seekable reader behind a [`BufReader`](std::io::BufReader)
/// and tracks the stream position, so it seeks only when a read does not start where the
/// last one ended — and then by a relative seek that keeps the buffer when the target is
/// already buffered. Sequential and nearby reads cost a buffer copy, not a system call.
///
/// Reads, values, errors, and [`bit_pos`](Source::bit_pos) match [`SeekReader`] over the
/// same bytes, provided `R` supports [`SeekFrom::Current`](std::io::SeekFrom::Current) as
/// well as `Start` (every `std` seekable does): a jump outside the buffer is a relative
/// seek, where `SeekReader` only ever seeks from the start. The difference is the ownership
/// contract:
///
/// - Use `BufSeekReader` when the reader is **exclusively owned** — a `File` you opened for
///   this parse, a `Cursor`. Pass the unbuffered reader; it supplies its own buffer.
/// - Use [`SeekReader`] when the cursor may be **shared** — a `&File` (every `&File` moves
///   the one OS file offset), a cloned handle, or a reader others also move. `SeekReader`
///   re-seeks to an absolute offset before every read, so interleaved users stay correct.
///
/// There is deliberately no `get_mut` and no `Clone`: either would let the stream move
/// behind the tracked position. Do not move the cursor through [`get_ref`](Self::get_ref)
/// either (e.g. reading a `&File`).
///
/// # Examples
///
/// ```
/// use bnb::{BitOrder, BufSeekReader, ByteOrder, Layout, Source};
/// use std::io::Cursor;
///
/// // A TIFF-like header: the byte order is known only at run time.
/// let bytes = vec![b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
/// let byte = if bytes[..2] == *b"II" { ByteOrder::Little } else { ByteOrder::Big };
/// let mut r = BufSeekReader::with_layout(Cursor::new(bytes), Layout { bit: BitOrder::Msb, byte });
///
/// r.seek_to_bit(2 * 8).unwrap();
/// assert_eq!(r.read::<u16>().unwrap(), 42);
/// assert_eq!(r.read::<u32>().unwrap(), 8); // continues from the buffer, no seek
/// ```
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct BufSeekReader<R> {
    /// The owned reader and its buffer.
    inner: std::io::BufReader<R>,
    /// The byte offset the buffered stream is logically at, or `None` when unknown (before
    /// the first read and after any I/O error), which forces an absolute seek.
    stream_pos: Option<u64>,
    /// The bit cursor; reads start here.
    bit_pos: usize,
    /// The bit and byte order values are read in.
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> BufSeekReader<R> {
    /// Wraps `inner` at bit 0, MSB-first big-endian, with a default-sized buffer.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps `inner` at bit 0 with the given [`Layout`] (which may be chosen at run time),
    /// with a default-sized buffer.
    #[must_use]
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner: std::io::BufReader::new(inner),
            stream_pos: None,
            bit_pos: 0,
            layout,
            depth: 0,
        }
    }

    /// Borrows the underlying reader. Do not move its cursor through this borrow (a
    /// `&File` can): the tracked position would go stale.
    #[must_use]
    pub fn get_ref(&self) -> &R {
        self.inner.get_ref()
    }

    /// Unwraps the underlying reader. Buffered-but-unread bytes are discarded, so its
    /// cursor is at an unspecified offset: seek it before reading.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }

    /// Positions the buffered stream at byte `target`: nothing when already there, a
    /// relative seek (which keeps buffered bytes in range) when the position is known, and
    /// an absolute seek otherwise.
    fn position(&mut self, target: u64) -> std::io::Result<()> {
        let delta = self
            .stream_pos
            .and_then(|pos| i64::try_from(i128::from(target) - i128::from(pos)).ok());
        match delta {
            Some(0) => Ok(()),
            Some(delta) => self.inner.seek_relative(delta),
            None => {
                std::io::Seek::seek(&mut self.inner, std::io::SeekFrom::Start(target)).map(drop)
            }
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> sealed::Sealed for BufSeekReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> Source for BufSeekReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let span = ByteSpan::new(self.bit_pos, n)?;
        let positioned = self.position(span.start);
        // Unknown until the read succeeds: a failed seek or `read_exact` leaves the offset
        // unspecified, so the next read re-seeks absolutely.
        self.stream_pos = None;
        positioned.map_err(|e| span.io_error(&e))?;
        let acc = span.read(&mut self.inner, self.layout.bit)?;
        self.stream_pos = Some(span.start + span.len as u64);
        self.bit_pos = span.end;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.bit_pos = pos; // the stream is positioned (if needed) on the next read
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekSource for BufSeekReader<R> {}
