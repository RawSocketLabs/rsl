//! The in-memory cursors: [`BitReader`] and [`BitWriter`].

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt;

use crate::field::{BitOrder, Bits, ByteOrder};

#[cfg(doc)]
use super::MAX_DECODE_DEPTH;
use super::layout::{bits_in, bytes_in, emit_bits, extract_bits};
use super::sink::{
    append_whole_bytes, position_overflow, write_by_bits, write_bytes_per_byte, writes_whole_bytes,
};
use super::source::{read_by_bits, read_bytes_per_byte, read_into_per_byte};
use super::{BitError, ErrorKind, Layout, Sink, Source, sealed};

/// A cursor that reads values at arbitrary bit offsets from a byte slice, in a
/// chosen [`BitOrder`] (MSB-first by default — `bit 0` is the high bit of byte 0,
/// the RFC/ETSI ASCII-art convention; LSB-first for serial/PHY layers).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, u4, u12};
///
/// let mut r = BitReader::new(&[0xAB, 0xCD]);
/// assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA)); // 4 bits
/// assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD)); // the next 12, straddling a byte
/// assert_eq!(r.remaining_bits(), 0);
/// ```
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) bit_pos: usize,
    pub(super) order: BitOrder,
    pub(super) byte: ByteOrder,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`].
    pub(super) depth: u16,
}

impl<'a> BitReader<'a> {
    /// Wraps `bytes`, positioned at bit 0, **MSB-first**, big-endian.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::with_order(bytes, BitOrder::Msb)
    }

    /// Wraps `bytes`, positioned at bit 0, in the given bit order (big-endian).
    #[must_use]
    pub fn with_order(bytes: &'a [u8], order: BitOrder) -> Self {
        Self::with_layout(
            bytes,
            Layout {
                bit: order,
                byte: ByteOrder::Big,
            },
        )
    }

    /// Wraps `bytes`, positioned at bit 0, in the given [`Layout`] (bit + byte order).
    #[must_use]
    pub fn with_layout(bytes: &'a [u8], layout: Layout) -> Self {
        Self {
            bytes,
            bit_pos: 0,
            order: layout.bit,
            byte: layout.byte,
            depth: 0,
        }
    }

    /// The current absolute bit offset.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.bit_pos
    }

    /// Bits not yet consumed.
    #[must_use]
    pub fn remaining_bits(&self) -> usize {
        bits_in(self.bytes.len()) - self.bit_pos
    }

    /// Borrows and consumes the next `n` whole bytes without copying.
    ///
    /// This is the exact-span boundary for formats whose signed or hashed representation must be
    /// retained byte-for-byte. Unlike [`Source::read_bytes`], the returned slice aliases the
    /// original input and therefore exists only on the slice-backed reader.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotByteAligned`] when the cursor is between bytes,
    /// [`ErrorKind::UnexpectedEof`] when fewer than `n` bytes remain, or
    /// [`ErrorKind::PositionOverflow`] when the requested end cannot be represented.
    pub fn read_slice(&mut self, n: usize) -> Result<&'a [u8], BitError> {
        if self.bit_pos % 8 != 0 {
            return Err(BitError::new(ErrorKind::NotByteAligned, self.bit_pos));
        }
        let start = self.bit_pos / 8;
        let end = start
            .checked_add(n)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        if end > bits_in(self.bytes.len()) / 8 {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: n.saturating_mul(8),
                    remaining: self.remaining_bits(),
                },
                self.bit_pos,
            ));
        }
        self.bit_pos = end * 8;
        Ok(&self.bytes[start..end])
    }

    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the reader's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::UnexpectedEof`] if fewer
    /// than `n` bits remain. Either carries the current bit offset.
    // Forced inline: it is a bounds check, the byte-aligned fast path and two calls, and a
    // generated decoder passes a constant width, so the check and the path fold at each
    // field. Left to its heuristics the optimizer flipped between builds (DESIGN §19).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §19")]
    #[inline(always)]
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let n = n as usize;
        if n > 128 || n > self.remaining_bits() {
            return Err(self.read_error(n));
        }
        let acc = extract_bits(self.bytes, self.bit_pos, n, self.order);
        self.bit_pos += n;
        Ok(acc)
    }

    /// The error for a refused [`read_bits`](Self::read_bits) of `n` bits: too wide, or
    /// past the end. Out of line and cold so the inlined read carries one call for both.
    #[cold]
    #[inline(never)]
    fn read_error(&self, n: usize) -> BitError {
        let kind = if n > 128 {
            ErrorKind::TooWide { width: n }
        } else {
            ErrorKind::UnexpectedEof {
                needed: n,
                remaining: self.remaining_bits(),
            }
        };
        BitError::new(kind, self.bit_pos)
    }

    /// Reads one [`Bits`] value of its declared width, applying the byte order to a
    /// byte-multiple value.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    #[inline]
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Source::read(self)
    }

    /// Moves the cursor to absolute bit `pos`. This needs no `Seek` trait — the whole
    /// buffer is in hand, so a seek is just cursor arithmetic. (Enables e.g. DNS
    /// name-compression pointers.)
    ///
    /// # Errors
    /// [`ErrorKind::UnexpectedEof`] if `pos` is past the end of the buffer.
    pub fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        let end = bits_in(self.bytes.len());
        if pos > end {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: pos,
                    remaining: end,
                },
                self.bit_pos,
            ));
        }
        self.bit_pos = pos;
        Ok(())
    }

    /// Advances the cursor to the next byte boundary (a no-op if already aligned).
    pub fn align_to_byte(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }
}

/// A sink that appends values at arbitrary bit offsets in a chosen [`BitOrder`]
/// (MSB-first by default), growing a byte buffer (the final partial byte is
/// zero-padded).
///
/// # Examples
///
/// ```
/// use bnb::{BitWriter, u4, u12};
///
/// let mut w = BitWriter::new();
/// w.write(u4::new(0xA)).unwrap();
/// w.write(u12::new(0xBCD)).unwrap();
/// assert_eq!(w.bit_len(), 16);
/// assert_eq!(w.into_bytes(), [0xAB, 0xCD]);
/// ```
#[derive(Default)]
pub struct BitWriter {
    pub(super) bytes: Vec<u8>,
    pub(super) bit_pos: usize,
    pub(super) order: BitOrder,
    pub(super) byte: ByteOrder,
    /// Optional per-encode scratch (see [`Sink::scratch`]); not part of the written bytes.
    pub(super) scratch: Option<Box<dyn Any + Send>>,
}

// Hand-written so the public `Clone`/`Debug` API survives the (un-`Clone`, un-`Debug`)
// `scratch` slot. Cloning starts a fresh encode session, so the scratch is **not**
// carried (a clone gets `None`); `Debug` reports only its presence.
impl Clone for BitWriter {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            bit_pos: self.bit_pos,
            order: self.order,
            byte: self.byte,
            scratch: None,
        }
    }
}

impl fmt::Debug for BitWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitWriter")
            .field("bytes", &self.bytes)
            .field("bit_pos", &self.bit_pos)
            .field("order", &self.order)
            .field("byte", &self.byte)
            .field("scratch", &self.scratch.is_some())
            .finish()
    }
}

impl BitWriter {
    /// An empty **MSB-first**, big-endian writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty writer in the given bit order (big-endian).
    #[must_use]
    pub fn with_order(order: BitOrder) -> Self {
        Self::with_layout(Layout {
            bit: order,
            byte: ByteOrder::Big,
        })
    }

    /// An empty writer in the given [`Layout`] (bit + byte order).
    #[must_use]
    pub fn with_layout(layout: Layout) -> Self {
        Self {
            bytes: Vec::new(),
            bit_pos: 0,
            order: layout.bit,
            byte: layout.byte,
            scratch: None,
        }
    }

    /// Attach a type-erased **scratch** value, reachable from any codec during the encode
    /// via [`Sink::scratch`] and recovered by `Any::downcast_mut`.
    ///
    /// The escape hatch for codecs that need mutable state shared across a whole message's
    /// fields — a back-reference / compression dictionary (e.g. DNS name compression). The
    /// scratch lives for the encode and is dropped by [`into_bytes`](Self::into_bytes); it
    /// is never written to the output and is not carried by [`Clone`]. It must be [`Send`],
    /// so a writer holding one can still move to another thread or task.
    ///
    /// ```
    /// use bnb::{BitWriter, Sink};
    ///
    /// let mut w = BitWriter::new().with_scratch(Box::new(0u32));
    /// if let Some(n) = w.scratch().and_then(|s| s.downcast_mut::<u32>()) {
    ///     *n += 1;
    /// }
    /// assert_eq!(w.scratch().and_then(|s| s.downcast_ref::<u32>()), Some(&1));
    /// ```
    #[must_use]
    pub fn with_scratch(mut self, scratch: Box<dyn Any + Send>) -> Self {
        self.scratch = Some(scratch);
        self
    }

    /// Bits written so far.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.bit_pos
    }

    /// Appends the low `n` (`<= 128`) bits of `value`, in the writer's bit order.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::PositionOverflow`] if the output
    /// would hold more bits than `usize` can count.
    #[inline]
    pub fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        let end = self
            .bit_pos
            .checked_add(n)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        emit_bits(&mut self.bytes, self.bit_pos, value, n, self.order);
        self.bit_pos = end;
        Ok(())
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order to
    /// a byte-multiple value.
    ///
    /// # Errors
    /// As [`write_bits`](Self::write_bits).
    #[inline]
    pub fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        Sink::write(self, value)
    }

    /// Consumes the writer, returning the packed bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl sealed::Sealed for BitReader<'_> {}

impl Source for BitReader<'_> {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    // Forced for the same reason as the inherent method it forwards to.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §19")]
    #[inline(always)]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        BitReader::read_bits(self, n)
    }
    // Forced inline: this is the call a generated decoder makes for each scalar field, and
    // out of line it costs a call and an 80-byte `Result` through memory (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        if !self.at_whole_bytes::<T>() {
            return read_by_bits(self);
        }
        let Some(value) = self.take_whole_bytes() else {
            return Err(self.read_error(T::BITS as usize));
        };
        Ok(value)
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
    #[inline]
    fn bit_order(&self) -> BitOrder {
        self.order
    }
    #[inline]
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        BitReader::seek_to_bit(self, pos)
    }
    #[inline]
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        match self.aligned_run(n) {
            Some(run) => Ok(run.to_vec()),
            None => read_bytes_per_byte(self, n),
        }
    }
    #[inline]
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        match self.aligned_run(buf.len()) {
            Some(run) => {
                buf.copy_from_slice(run);
                Ok(())
            }
            None => read_into_per_byte(self, buf),
        }
    }
    #[inline]
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        BitReader::aligned_run(self, n)
    }
}

impl<'a> BitReader<'a> {
    /// Whether a `T` read here is whole bytes from a byte-aligned cursor, the case
    /// [`take_whole_bytes`](Self::take_whole_bytes) serves.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    pub(super) fn at_whole_bytes<T: Bits>(&self) -> bool {
        T::BITS % 8 == 0 && T::BITS <= 128 && self.bit_pos % 8 == 0
    }

    /// Reads a `T` as [`at_whole_bytes`](Self::at_whole_bytes) allows; `None`, consuming
    /// nothing, when fewer bytes remain. Aligned whole bytes are the value big-endian or
    /// little-endian whatever the bit order (each order's natural layout, swapped or not:
    /// `apply_byte_order`), so this is one load and at most a swap.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    pub(super) fn take_whole_bytes<T: Bits>(&mut self) -> Option<T> {
        debug_assert!(self.at_whole_bytes::<T>());
        let n = (T::BITS / 8) as usize;
        let start = self.bit_pos / 8;
        let end = bytes_in(self.bytes.len());
        let run = self.bytes[..end].get(start..start + n)?;
        let mut wide = [0; 16];
        let raw = match T::__BNB_BYTE_ORDER.unwrap_or(self.byte) {
            ByteOrder::Big => {
                wide[16 - n..].copy_from_slice(run);
                u128::from_be_bytes(wide)
            }
            ByteOrder::Little => {
                wide[..n].copy_from_slice(run);
                u128::from_le_bytes(wide)
            }
        };
        self.bit_pos += T::BITS as usize;
        Some(T::from_bits(raw))
    }

    // Consumes the next `n` bytes when the cursor is byte-aligned and all of them remain.
    // Whole aligned bytes read identically in both bit orders, so this is a plain copy.
    #[inline]
    pub(super) fn aligned_run(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.bit_pos % 8 != 0 {
            return None;
        }
        let start = self.bit_pos / 8;
        let end = start
            .checked_add(n)
            .filter(|&end| end <= bits_in(self.bytes.len()) / 8)?;
        let run = self.bytes.get(start..end)?;
        self.bit_pos = end * 8;
        Some(run)
    }
}

impl sealed::Sealed for BitWriter {}

impl Sink for BitWriter {
    #[inline]
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        BitWriter::write_bits(self, value, n)
    }
    // Forced inline: this is the call a generated encoder makes for each scalar field. A
    // whole-byte value at a byte-aligned cursor, which is always the end of the buffer, is
    // appended in one step; any other write inlines the bit path (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        if !writes_whole_bytes::<T>(self.bit_pos) {
            return write_by_bits(self, value);
        }
        let Some(end) = self.bit_pos.checked_add(T::BITS as usize) else {
            return Err(position_overflow(self.bit_pos));
        };
        // An aligned cursor is the end of the buffer: no write leaves it anywhere else.
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let out = &mut self.bytes;
        append_whole_bytes(value, self.byte, |bytes| out.extend_from_slice(bytes));
        self.bit_pos = end;
        Ok(())
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
    #[inline]
    fn bit_order(&self) -> BitOrder {
        self.order
    }
    #[inline]
    fn scratch(&mut self) -> Option<&mut dyn Any> {
        // The stored box is `dyn Any + Send`; the trait hands out plain `dyn Any`.
        self.scratch
            .as_deref_mut()
            .map(|scratch| scratch as &mut dyn Any)
    }
    // An aligned cursor sits exactly at the end of the buffer, and whole aligned bytes write
    // identically in both bit orders, so the run is appended as-is. It fails only when the
    // output would hold more bits than `usize` can count.
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        if self.bit_pos % 8 != 0 {
            return write_bytes_per_byte(self, bytes);
        }
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let end = bytes
            .len()
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        self.bytes.extend_from_slice(bytes);
        self.bit_pos = end;
        Ok(())
    }
    // As `write_bytes`: an aligned cursor sits at the end of the buffer, so the tail is new.
    #[inline]
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        if self.bit_pos % 8 != 0 {
            return None;
        }
        let end = n
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))?;
        debug_assert_eq!(self.bit_pos / 8, self.bytes.len());
        let start = self.bytes.len();
        self.bytes.resize(start + n, 0);
        self.bit_pos = end;
        Some(&mut self.bytes[start..])
    }
}

#[cfg(test)]
mod unit {
    #[cfg(feature = "bytes")]
    use crate::bitstream::BytesWriter;
    use crate::{u4, u12};

    use super::*;

    #[test]
    fn writers_stay_send_so_an_encode_can_move_across_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<BitWriter>();
        #[cfg(feature = "bytes")]
        assert_send::<BytesWriter>();
    }

    #[test]
    fn unaligned_round_trip() {
        let mut w = BitWriter::new();
        w.write(u4::new(0xA)).unwrap();
        w.write(u12::new(0xBCD)).unwrap();
        assert_eq!(w.bit_len(), 16);
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xAB, 0xCD]);

        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
        assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD));
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn eof_is_an_error_not_a_panic() {
        let mut r = BitReader::new(&[0xFF]);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xF));
        let err = r.read_bits(8).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnexpectedEof {
                needed: 8,
                remaining: 4
            }
        );
        assert_eq!(err.at, 4, "error records the bit offset");
        assert!(err.field.is_none(), "no field context at the reader level");
    }

    #[test]
    fn too_wide_is_rejected() {
        let mut r = BitReader::new(&[0u8; 32]);
        let err = r.read_bits(129).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TooWide { width: 129 });
    }

    #[test]
    fn writer_with_order_lsb_packs_first_field_in_the_low_bits() {
        let mut w = BitWriter::with_order(BitOrder::Lsb);
        w.write(u4::new(0xA)).unwrap(); // -> low nibble
        w.write(u4::new(0xB)).unwrap(); // -> high nibble
        assert_eq!(w.into_bytes(), [0xBA]);
    }

    #[test]
    fn cursor_layout_matrix_bit_and_byte_order_are_independent() {
        // The two axes — `bit` (how a sub-byte field packs) and `byte` (how a byte-multiple
        // value serializes) — must compose independently at the cursor level (`extract_bits`/
        // `emit_bits`/`apply_byte_order`). Write a nibble pair (bit-order sensitive) then a u16
        // word (byte-order sensitive) under each of the four layouts.
        let combos = [
            (BitOrder::Msb, ByteOrder::Big),
            (BitOrder::Msb, ByteOrder::Little),
            (BitOrder::Lsb, ByteOrder::Big),
            (BitOrder::Lsb, ByteOrder::Little),
        ];
        let mut encs = Vec::new();
        for (bit, byte) in combos {
            let layout = Layout { bit, byte };
            let mut w = BitWriter::with_layout(layout);
            w.write(u4::new(0xA)).unwrap();
            w.write(u4::new(0xB)).unwrap();
            w.write(0x1234u16).unwrap();
            let bytes = w.into_bytes();
            // Every layout round-trips through a reader with the same layout.
            let mut r = BitReader::with_layout(&bytes, layout);
            assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
            assert_eq!(r.read::<u4>().unwrap(), u4::new(0xB));
            assert_eq!(r.read::<u16>().unwrap(), 0x1234);
            encs.push(bytes);
        }
        // All four corners are golden under the natural-layout rule (`apply_byte_order`):
        // each bit order has a natural byte layout — big-endian under MSB, little-endian
        // under LSB (the DBC-Intel layout) — and the byte-order knob swaps a byte-multiple
        // value only when it differs from that natural layout.
        assert_eq!(encs[0], [0xAB, 0x12, 0x34]); // msb / big    (natural — no swap)
        assert_eq!(encs[1], [0xAB, 0x34, 0x12]); // msb / little (differs — word swaps)
        assert_eq!(encs[2], [0xBA, 0x12, 0x34]); // lsb / big    (differs — word swaps)
        assert_eq!(encs[3], [0xBA, 0x34, 0x12]); // lsb / little (natural — no swap)
        // Flipping byte order changes only the word's bytes, never the nibble byte.
        assert_eq!(encs[0][0], encs[1][0]);
        assert_ne!(encs[0][1..], encs[1][1..]);
        assert_eq!(encs[2][0], encs[3][0]);
        assert_ne!(encs[2][1..], encs[3][1..]);
        // All four corners are pairwise distinct — neither axis aliases the other.
        for i in 0..encs.len() {
            for j in (i + 1)..encs.len() {
                assert_ne!(encs[i], encs[j], "layout corners {i} and {j} alias");
            }
        }
    }

    #[test]
    fn write_bits_rejects_over_128() {
        let mut w = BitWriter::new();
        assert_eq!(
            w.write_bits(0, 129).unwrap_err().kind,
            ErrorKind::TooWide { width: 129 }
        );
    }

    #[test]
    fn bit_reader_borrows_exact_aligned_input_spans() {
        let bytes = [0x30, 0x03, 0x02, 0x01, 0x07];
        let mut source = BitReader::new(&bytes);
        assert_eq!(source.read_slice(2).unwrap(), &bytes[..2]);
        assert_eq!(source.read_slice(3).unwrap(), &bytes[2..]);
        assert_eq!(source.remaining_bits(), 0);

        let mut unaligned = BitReader::new(&bytes);
        unaligned.read_bits(1).unwrap();
        assert_eq!(
            unaligned.read_slice(1).unwrap_err().kind,
            ErrorKind::NotByteAligned
        );
    }

    #[test]
    fn bulk_bytes_round_trip() {
        let mut w = BitWriter::new();
        w.write_bytes(&[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xDE, 0xAD, 0xBE, 0xEF]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read_bytes(4).unwrap(), [0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(r.remaining_bits(), 0);
    }

    #[test]
    fn oversized_custom_bits_reports_width_error_before_byte_swap() {
        #[derive(Clone, Copy)]
        struct InvalidWidth;
        impl Bits for InvalidWidth {
            const BITS: u32 = 136;
            fn into_bits(self) -> u128 {
                0
            }
            fn from_bits(_: u128) -> Self {
                Self
            }
        }

        let mut writer = BitWriter::with_layout(Layout {
            bit: BitOrder::Msb,
            byte: ByteOrder::Little,
        });
        assert_eq!(
            writer.write(InvalidWidth).unwrap_err().kind,
            ErrorKind::TooWide { width: 136 }
        );
        assert_eq!(writer.into_bytes(), b"");
    }

    #[test]
    fn bulk_bytes_work_at_a_bit_offset() {
        // Bytes need not be aligned: write a nibble, then bytes straddling.
        let mut w = BitWriter::new();
        w.write(u4::new(0xF)).unwrap();
        w.write_bytes(&[0xAB, 0xCD]).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(bytes, [0xFA, 0xBC, 0xD0]);
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.read::<u4>().unwrap(), u4::new(0xF));
        assert_eq!(r.read_bytes(2).unwrap(), [0xAB, 0xCD]);
    }

    #[test]
    fn read_bytes_hostile_length_is_eof_not_alloc() {
        // A huge n against a 2-byte source: push-per-byte means a fast EOF, no
        // pre-allocation from the untrusted length.
        let mut r = BitReader::new(&[0x01, 0x02]);
        let err = r.read_bytes(usize::MAX).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    #[test]
    fn read_into_fills_and_errors_short() {
        let mut r = BitReader::new(&[0x0A, 0x0B, 0x0C]);
        let mut buf = [0u8; 3];
        r.read_into(&mut buf).unwrap();
        assert_eq!(buf, [0x0A, 0x0B, 0x0C]);

        let mut r = BitReader::new(&[0x0A]);
        let mut buf = [0u8; 3];
        let err = r.read_into(&mut buf).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }
}
