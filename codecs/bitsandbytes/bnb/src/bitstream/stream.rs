//! Sources over a forward-only `Read`: [`StreamBitReader`] reads once, [`BufSource`]
//! retains a bounded window so it can seek back.

use crate::field::{BitOrder, Bits, ByteOrder};

use super::layout::{bits_in, extract_bits, value_byte_order};
use super::source::{read_bytes_per_byte, read_into_per_byte};
#[cfg(doc)]
use super::{BitBuf, BitEncode, BitReader, MAX_DECODE_DEPTH, SeekReader};
use super::{BitError, ErrorKind, Layout, SeekSource, Source, sealed};

/// A *forward-only* bit reader over any [`std::io::Read`] — the streaming counterpart
/// to the in-memory [`BitReader`], for a stream you read once and don't seek.
///
/// It is bounded on `Read` **only, not `Seek`**, so it works over inputs that can't
/// seek (a socket, or a `&[u8]`, which is `Read` but not `Seek`). A message that needs
/// to seek (`#[br(restore_position)]`) won't decode through it — use a [`BufSource`] or
/// [`SeekReader`] for that. Reads up to 128 bits per call (the [`Source`] width
/// ceiling); running out mid-message yields [`ErrorKind::Incomplete`]. This reader is
/// **not transactional**: input already consumed during a failed field cannot be replayed.
/// Use [`BitBuf::try_pull`] for buffered, retryable message decoding. Non-EOF I/O failures
/// keep their original [`ErrorKind::Io`] kind.
///
/// Whole bytes read from between two bytes (a `u32`, a `[u8; N]`, a counted `Vec<u8>`) are
/// asked of the reader together, through [`Read::read`](std::io::Read::read), again while
/// it returns fewer than were asked for. A read that starts or ends inside a byte takes the
/// stream one byte at a time, through [`Read::read_exact`](std::io::Read::read_exact).
/// Either way the reader is never asked for more than the field in hand needs.
///
/// # Examples
///
/// ```
/// use bnb::{bin, StreamBitReader};
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// // `&[u8]` is `Read` but not `Seek` — exactly the forward-only case.
/// let data: &[u8] = &[0x12, 0x34, 0x56, 0x78];
/// let mut s = StreamBitReader::new(data);
/// assert_eq!(Word::decode(&mut s).unwrap(), Word { value: 0x1234_5678 });
/// ```
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct StreamBitReader<R> {
    pub(super) inner: R,
    /// Leftover bits from the last partially-consumed byte, right-aligned in the low
    /// `lead_bits` bits. Always fewer than 8. Consumed high-first under `Msb`, low-first
    /// under `Lsb`.
    lead: u32,
    lead_bits: u32,
    /// Total bits consumed so far (for position-aware errors).
    pub(super) pos: usize,
    /// The bit/byte order this source decodes in. `read_bits` honors the **bit** order
    /// (like every other `Source`); **byte** order is applied to whole values by
    /// [`read`](Self::read) / `Source::read`.
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> StreamBitReader<R> {
    /// Wraps a byte source, decoding in the default **MSB-first, big-endian** order. For a
    /// `#[bin(little)]`/`bits = lsb` message, use [`with_layout`](Self::with_layout) with
    /// that type's [`LAYOUT`](BitEncode::LAYOUT), or the decode reads the wrong order.
    pub fn new(inner: R) -> Self {
        Self::with_layout(inner, Layout::default())
    }

    /// Wraps a byte source, decoding in the given [`Layout`] (bit + byte order) — pass a
    /// message's `LAYOUT` so a non-default-order `#[bin]` type round-trips over a stream.
    pub fn with_layout(inner: R, layout: Layout) -> Self {
        Self {
            inner,
            lead: 0,
            lead_bits: 0,
            pos: 0,
            layout,
            depth: 0,
        }
    }

    /// The total number of bits consumed so far.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    /// Reads `n` (`<= 128`) bits in the source's [bit order](Layout), pulling bytes from
    /// the source as needed. This matches every other [`Source`]: byte order is applied to
    /// whole values by [`read`](Self::read) / `Source::read`, not here.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::Incomplete`] if the
    /// source runs out mid-field; [`ErrorKind::PositionOverflow`] once the stream has
    /// delivered more bits than `usize` can count. Each carries the bit offset. A failed read may
    /// have consumed input; do not retry the field on this reader. Use `BitBuf`
    /// when the caller needs transactional incremental attempts.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if self.lead_bits != 0 || n % 8 != 0 || n > 128 {
            return self.read_bits_per_byte(n);
        }
        // Whole bytes from between two bytes: asked of the reader together. Bytes in
        // stream order are the value most significant first under `Msb` and least
        // significant first under `Lsb`, which is what the loop below accumulates.
        let at = self.pos;
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        let mut bytes = [0u8; 16];
        let count = (n / 8) as usize;
        let wanted = match self.layout.bit {
            BitOrder::Msb => &mut bytes[16 - count..],
            BitOrder::Lsb => &mut bytes[..count],
        };
        if let (_, Some(stopped)) = pull(&mut self.inner, wanted) {
            return Err(BitError::new(stopped.in_a_stream(), at));
        }
        self.pos = end;
        Ok(match self.layout.bit {
            BitOrder::Msb => u128::from_be_bytes(bytes),
            BitOrder::Lsb => u128::from_le_bytes(bytes),
        })
    }

    /// [`read_bits`](Self::read_bits) one byte of the stream at a time: the form that
    /// serves a read starting or ending inside a byte.
    pub(super) fn read_bits_per_byte(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.pos,
            ));
        }
        let at = self.pos;
        let end = at
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, at))?;
        // Consume the leftover bits of the current byte, then whole bytes, honoring the
        // source's bit order so a streaming read matches the slice-backed sources exactly
        // (`extract_bits`). The accumulator never holds more than `n` (<= 128) bits, so it
        // can't overflow.
        let mut result: u128 = 0;
        let mut need = n;
        let mut placed = 0u32; // bits already placed (the LSB-path result cursor)
        while need > 0 {
            if self.lead_bits == 0 {
                let mut b = [0u8; 1];
                if let Err(error) = self.inner.read_exact(&mut b) {
                    return Err(BitError::new(stream_failure(error.kind()), at));
                }
                self.lead = u32::from(b[0]);
                self.lead_bits = 8;
            }
            let take = need.min(self.lead_bits);
            match self.layout.bit {
                // MSB-first: consume the *high* `take` bits, accumulate high-to-low.
                BitOrder::Msb => {
                    let shift = self.lead_bits - take;
                    let chunk = (self.lead >> shift) & ((1u32 << take) - 1);
                    result = (result << take) | u128::from(chunk);
                    self.lead_bits -= take;
                    self.lead &= (1u32 << self.lead_bits) - 1; // keep the unconsumed low bits
                }
                // LSB-first: consume the *low* `take` bits, place them at the bit cursor.
                BitOrder::Lsb => {
                    let chunk = self.lead & ((1u32 << take) - 1);
                    result |= u128::from(chunk) << placed;
                    self.lead >>= take; // drop the consumed low bits
                    self.lead_bits -= take;
                }
            }
            placed += take;
            need -= take;
        }
        self.pos = end;
        Ok(result)
    }

    /// Reads one [`Bits`] value (width `<= 128`) of its declared width, applying the
    /// source's byte order to whole values — matching [`BitReader::read`] and
    /// `Source::read`, so `sr.read::<T>()` agrees with `T::decode(&mut sr)`.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(value_byte_order::<T>(
            raw,
            self.layout.bit,
            self.layout.byte,
        )))
    }

    /// Reads whole bytes into `dst` from a byte-aligned cursor and counts them, stopping
    /// where a byte-by-byte read would: at the end of the input, at a reader error, or at
    /// the last byte the position can count. Says what stopped it short, if anything did.
    fn fill(&mut self, dst: &mut [u8]) -> Option<BitError> {
        debug_assert_eq!(self.lead_bits, 0);
        let fits = dst.len().min((usize::MAX - self.pos) / 8);
        let (got, stopped) = pull(&mut self.inner, &mut dst[..fits]);
        self.pos += got * 8;
        stopped
            .map(Stopped::in_a_stream)
            // Lazily: an error kind built and dropped unused is a call into its drop code.
            .or_else(|| (fits < dst.len()).then_some(ErrorKind::PositionOverflow))
            .map(|kind| BitError::new(kind, self.pos))
    }
}

/// Why a reader gave fewer bytes than it was asked for.
#[cfg(feature = "std")]
enum Stopped {
    /// It has no more: a read returned nothing.
    End,
    Failed(std::io::ErrorKind),
}

#[cfg(feature = "std")]
impl Stopped {
    /// What it means under a [`StreamBitReader`], where the end of the input is a message
    /// that is not all there yet.
    fn in_a_stream(self) -> ErrorKind {
        match self {
            Stopped::End => ErrorKind::Incomplete { needed: None },
            Stopped::Failed(kind) => stream_failure(kind),
        }
    }
}

/// Reads from `inner` until `dst` is full, as reading it one byte at a time would: an
/// interrupted read is tried again, the end of the input or an error stops it. Returns how
/// many bytes came and what stopped them, if anything did.
#[cfg(feature = "std")]
fn pull<R: std::io::Read>(inner: &mut R, dst: &mut [u8]) -> (usize, Option<Stopped>) {
    let mut got = 0;
    while got < dst.len() {
        match inner.read(&mut dst[got..]) {
            Ok(0) => return (got, Some(Stopped::End)),
            // A reader cannot have filled more than it was lent.
            Ok(count) => got += count.min(dst.len() - got),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return (got, Some(Stopped::Failed(error.kind()))),
        }
    }
    (got, None)
}

/// What a failed read of the stream under a [`StreamBitReader`] means: the end of the
/// input is a message that is not all there yet, anything else is the reader's own error.
#[cfg(feature = "std")]
fn stream_failure(kind: std::io::ErrorKind) -> ErrorKind {
    if kind == std::io::ErrorKind::UnexpectedEof {
        ErrorKind::Incomplete { needed: None }
    } else {
        ErrorKind::Io(kind)
    }
}

/// The most bytes a byte run asks a reader for before any have arrived; after that, as
/// many again as it has. A count read off the wire therefore allocates for input that
/// came, not for what it claims.
#[cfg(feature = "std")]
pub(super) const RUN_STEP: usize = 4096;

#[cfg(feature = "std")]
impl<R: std::io::Read> sealed::Sealed for StreamBitReader<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for StreamBitReader<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        StreamBitReader::read_bits(self, n)
    }
    /// A run from a byte-aligned cursor asks the reader for as much as it still needs at
    /// a time; values, errors and the bytes consumed are those of reading it byte by byte.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if self.lead_bits != 0 {
            return read_bytes_per_byte(self, n);
        }
        let mut bytes = Vec::new();
        while bytes.len() < n {
            let have = bytes.len();
            bytes.resize(have + (n - have).min(have.max(RUN_STEP)), 0);
            if let Some(error) = self.fill(&mut bytes[have..]) {
                return Err(error);
            }
        }
        Ok(bytes)
    }
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if self.lead_bits != 0 {
            return read_into_per_byte(self, buf);
        }
        match self.fill(buf) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn bit_pos(&self) -> usize {
        self.pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
}

/// A **seekable** [`Source`] over a forward `Read` (a socket): it *retains* the bytes
/// it has read, so a seek-using message (`restore_position`) works over a non-seekable
/// stream by seeking within the retained buffer, reading more on demand. It is
/// **bounded** — a retention `cap` (default 64 KiB) past which it errors
/// [`ErrorKind::BufferFull`] rather than buffering unboundedly. The
/// "continuously-receiving peer that also needs to seek" case. Once the reader returns
/// EOF, a read past the retained bytes is [`ErrorKind::UnexpectedEof`], as on a slice;
/// for input that may still grow, push into a [`BitBuf`] instead.
///
/// **Reclaim is explicit.** Every byte read stays retained, and counts against the cap,
/// until [`compact`](Self::compact) discards the bytes before the cursor. A loop decoding
/// message after message from one `BufSource` must call it between messages, or the stream
/// ends in [`ErrorKind::BufferFull`] once `cap` bytes have gone by. Nothing is reclaimed
/// automatically, because a later seek may still want those bytes. Positions
/// ([`bit_pos`](Source::bit_pos), [`seek_to_bit`](Source::seek_to_bit), error offsets) count
/// from the start of the stream and do not move when bytes are discarded.
///
/// # Examples
///
/// ```
/// use bnb::{bin, BufSource, Source};
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// let wire = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
/// let mut src = BufSource::with_capacity(&wire[..], 4); // any `Read`; retain 4 bytes at most
/// assert_eq!(Word::decode(&mut src).unwrap(), Word { value: 0x1234_5678 });
/// src.compact(); // done with that message: release its bytes
/// assert_eq!(Word::decode(&mut src).unwrap(), Word { value: 0x9ABC_DEF0 });
/// assert_eq!(src.bit_pos(), 64, "positions stay stream-absolute");
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct BufSource<R> {
    pub(super) inner: R,
    /// The retained bytes: stream bytes `base..base + buf.len()`.
    pub(super) buf: Vec<u8>,
    /// Stream offset, in bytes, of `buf[0]`: how much [`compact`](Self::compact) has discarded.
    pub(super) base: usize,
    /// The cursor, in bits from the start of the stream; never before `base`.
    pub(super) bit_pos: usize,
    cap: usize,
    layout: Layout,
    eof: bool,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    depth: u16,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> BufSource<R> {
    /// Wraps `inner` with the default 64 KiB retention cap, MSB-first big-endian.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_capacity(inner, 64 * 1024)
    }

    /// Wraps `inner` with a retention `cap` (bytes), MSB-first big-endian.
    #[must_use]
    pub fn with_capacity(inner: R, cap: usize) -> Self {
        Self::with_capacity_and_layout(inner, cap, Layout::default())
    }

    /// Wraps `inner` with a retention `cap` (bytes) and [`Layout`].
    #[must_use]
    pub fn with_capacity_and_layout(inner: R, cap: usize, layout: Layout) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            base: 0,
            bit_pos: 0,
            cap,
            layout,
            eof: false,
            depth: 0,
        }
    }

    /// Discards the retained bytes before the cursor's byte, keeping a partially consumed
    /// byte, so they stop counting against the retention cap. Call it between messages in a
    /// long-lived decode loop; the allocation is reused.
    ///
    /// Positions stay stream-absolute: [`bit_pos`](Source::bit_pos) is unchanged. The
    /// discarded bytes are gone, so a later [`seek_to_bit`](Source::seek_to_bit) before the
    /// retained bytes fails with [`ErrorKind::NotSeekable`] — do not call this while a
    /// `restore_position` or pointer of the message in progress may still seek back.
    ///
    /// Because positions never rewind, the stream as a whole is limited to `usize::MAX`
    /// bits: on a 32-bit target a compacting loop ends in [`ErrorKind::PositionOverflow`]
    /// after 512 MiB. Start a new `BufSource` over the reader before then.
    pub fn compact(&mut self) {
        // A forward seek can leave the cursor past everything read so far.
        let whole = (self.bit_pos / 8 - self.base).min(self.buf.len());
        self.buf.drain(..whole);
        self.base += whole;
    }

    /// Reads from `inner` until the retained bytes reach stream byte `byte_end` (or
    /// EOF/cap), never past it. An [`Interrupted`](std::io::ErrorKind::Interrupted) read is
    /// retried; any other reader error is [`ErrorKind::Io`].
    fn fill_to(&mut self, byte_end: usize) -> Result<(), BitError> {
        while self.base + self.buf.len() < byte_end && !self.eof {
            if self.buf.len() >= self.cap {
                return Err(BitError::new(
                    ErrorKind::BufferFull { cap: self.cap },
                    self.bit_pos,
                ));
            }
            let want = (byte_end - self.base - self.buf.len())
                .min(self.cap - self.buf.len())
                .min(self.buf.len().max(RUN_STEP));
            // Cleared once for the step, however many reads the reader takes to fill it.
            let start = self.buf.len();
            self.buf.resize(start + want, 0);
            let (got, stopped) = pull(&mut self.inner, &mut self.buf[start..]);
            self.buf.truncate(start + got);
            match stopped {
                None => {}
                Some(Stopped::End) => self.eof = true,
                Some(Stopped::Failed(kind)) => {
                    return Err(BitError::new(ErrorKind::Io(kind), self.bit_pos));
                }
            }
        }
        Ok(())
    }

    /// Retains and consumes up to `n` whole bytes from a byte-aligned cursor, stopping
    /// where `n` single-byte reads would. Returns where the bytes it got lie in `buf`, and
    /// what stopped it short, if anything did.
    fn take_run(&mut self, n: usize) -> (core::ops::Range<usize>, Option<BitError>) {
        debug_assert_eq!(self.bit_pos % 8, 0);
        let start = self.bit_pos / 8;
        let fits = n.min((usize::MAX - self.bit_pos) / 8);
        let (mut end, mut stopped) = (start, None);
        // A run of no bytes reads nothing, not even what a forward seek skipped.
        if fits > 0 {
            let filled = self.fill_to(start + fits);
            let retained = self.base + self.buf.len();
            end = retained.clamp(start, start + fits);
            stopped = match filled {
                Err(error) => Some(error.kind),
                Ok(()) if end < start + fits => Some(ErrorKind::UnexpectedEof {
                    needed: 8,
                    remaining: 0,
                }),
                Ok(()) => None,
            };
        }
        if stopped.is_none() && fits < n {
            stopped = Some(ErrorKind::PositionOverflow);
        }
        self.bit_pos = end * 8;
        // After a forward seek the cursor can lie past everything retained.
        let at = (start - self.base).min(self.buf.len());
        (
            at..at + (end - start),
            stopped.map(|kind| BitError::new(kind, self.bit_pos)),
        )
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> sealed::Sealed for BufSource<R> {}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for BufSource<R> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.bit_pos,
            ));
        }
        let end = self
            .bit_pos
            .checked_add(n as usize)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        let byte_end = end.div_ceil(8);
        self.fill_to(byte_end)?;
        let retained_end = self.base + self.buf.len();
        if retained_end < byte_end {
            // `fill_to` stops short only at the reader's EOF, so the input is finite and
            // finished: report it as a slice would, letting dispatch fall through.
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: n as usize,
                    remaining: bits_in(retained_end).saturating_sub(self.bit_pos),
                },
                self.bit_pos,
            ));
        }
        // `buf` starts at stream byte `base`, and the cursor is never before it.
        let at = self.bit_pos - self.base * 8;
        let acc = extract_bits(&self.buf, at, n as usize, self.layout.bit);
        self.bit_pos += n as usize;
        Ok(acc)
    }
    /// A run from a byte-aligned cursor is retained in as few reads as the reader allows
    /// and copied out once; values, errors, the cursor and what is retained are those of
    /// reading it byte by byte.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if self.bit_pos % 8 != 0 {
            return read_bytes_per_byte(self, n);
        }
        match self.take_run(n) {
            (_, Some(error)) => Err(error),
            (run, None) => Ok(self.buf[run].to_vec()),
        }
    }
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if self.bit_pos % 8 != 0 {
            return read_into_per_byte(self, buf);
        }
        let (run, stopped) = self.take_run(buf.len());
        buf[..run.len()].copy_from_slice(&self.buf[run]);
        match stopped {
            Some(error) => Err(error),
            None => Ok(()),
        }
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
    /// Moves the cursor to stream-absolute bit `pos`.
    ///
    /// # Errors
    /// [`ErrorKind::NotSeekable`] if `pos` is before the retained bytes, which
    /// [`compact`](BufSource::compact) discarded; the cursor does not move.
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        // Seek within the retained buffer; a later read fills more on demand.
        // Backward seeks (`restore_position`) hit already-retained bytes.
        if pos / 8 < self.base {
            return Err(BitError::new(ErrorKind::NotSeekable, self.bit_pos));
        }
        self.bit_pos = pos;
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> SeekSource for BufSource<R> {}

#[cfg(test)]
mod unit {
    use crate::bitstream::{BitReader, BitWriter};

    use super::*;

    #[test]
    fn writer_and_stream_positions_fail_at_the_bound_instead_of_wrapping() {
        let mut w = BitWriter::new();
        w.bit_pos = usize::MAX - 3;
        assert_eq!(
            w.write_bits(0, 8).unwrap_err().kind,
            ErrorKind::PositionOverflow
        );
        assert_eq!(w.bit_pos, usize::MAX - 3);

        let mut r = StreamBitReader::new(&[0xFFu8][..]);
        r.pos = usize::MAX - 3;
        assert_eq!(
            r.read_bits(8).unwrap_err().kind,
            ErrorKind::PositionOverflow
        );
        assert_eq!(r.pos, usize::MAX - 3);
    }

    #[test]
    fn stream_reader_matches_slice_up_to_128_bits() {
        // The `Source` contract allows reads up to 128 bits; the forward streaming
        // reader must agree with the slice reader across the whole range, including
        // wide (> 64-bit) and byte-straddling reads.
        let bytes: Vec<u8> = (0u8..16).collect(); // 0x00 01 02 … 0F

        // A single 128-bit read.
        let mut s = StreamBitReader::new(&bytes[..]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(s.read_bits(128).unwrap(), r.read_bits(128).unwrap());

        // A 100-bit then 28-bit split (each crosses byte boundaries and the second
        // starts mid-byte, exercising the leftover-bits path).
        let mut s = StreamBitReader::new(&bytes[..]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(s.read_bits(100).unwrap(), r.read_bits(100).unwrap());
        assert_eq!(s.read_bits(28).unwrap(), r.read_bits(28).unwrap());

        // Over-wide is rejected at 128 now, not 64.
        let mut s = StreamBitReader::new(&bytes[..]);
        assert_eq!(
            s.read_bits(65).unwrap(),
            BitReader::new(&bytes).read_bits(65).unwrap(),
            "a 65-bit read used to be rejected"
        );
        let mut s = StreamBitReader::new(&bytes[..]);
        assert_eq!(
            s.read_bits(129).unwrap_err().kind,
            ErrorKind::TooWide { width: 129 }
        );
    }

    #[test]
    fn stream_reader_honors_every_layout() {
        // The streaming reader must agree with the slice reader in *all four* bit/byte
        // orders — not just the default `Msb`/`Big`. (An earlier `read_bits` hardcoded
        // MSB-first extraction, so `Lsb` layouts decoded wrong here alone; and the
        // inherent `read` skipped byte order, so it disagreed with `T::decode`.)
        let bytes: Vec<u8> = (0u8..16).collect();
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };

                // `read_bits` parity across widths + a mid-byte split (byte-aligned and not).
                let mut s = StreamBitReader::with_layout(&bytes[..], layout);
                let mut r = BitReader::with_layout(&bytes, layout);
                for n in [12u32, 20, 96] {
                    assert_eq!(
                        s.read_bits(n).unwrap(),
                        r.read_bits(n).unwrap(),
                        "read_bits({n}) diverged for {bit:?}/{byte:?}"
                    );
                }

                // The inherent `read` must apply byte order, matching the slice reader's
                // typed read (and therefore `T::decode`).
                let mut s = StreamBitReader::with_layout(&bytes[..], layout);
                let mut r = BitReader::with_layout(&bytes, layout);
                assert_eq!(
                    s.read::<u16>().unwrap(),
                    r.read::<u16>().unwrap(),
                    "read::<u16> diverged for {bit:?}/{byte:?}"
                );
                assert_eq!(
                    s.read::<u32>().unwrap(),
                    r.read::<u32>().unwrap(),
                    "read::<u32> diverged for {bit:?}/{byte:?}"
                );
            }
        }
    }
}
