//! `bytes`-crate adapters (the `bytes` feature): own a `Bytes` frame to decode
//! (zero copy), encode into an owned buffer you `freeze()` to a `Bytes` — the
//! async/tokio framing case. Off by default so the core stays dependency-light.

use alloc::vec::Vec;

use crate::field::{BitOrder, Bits, ByteOrder};

#[cfg(feature = "tokio")]
use super::ErrorKind;
#[cfg(feature = "tokio")]
use super::layout::{Appendable, emit_bits};
#[cfg(feature = "tokio")]
use super::sink::{
    append_whole_bytes, position_overflow, write_by_bits, write_bytes_per_byte, writes_whole_bytes,
};
use super::source::{aligned_run_at, read_via_slice};
use super::{BitError, BitReader, BitWriter, Layout, SeekSource, Sink, Source};

/// A [`SeekSource`] that **owns** a `bytes::Bytes` frame (no
/// borrow), decoding bits from it. Constructing it from a `Bytes` is a refcount
/// bump (zero copy).
#[derive(Clone, Debug)]
pub struct BytesReader {
    data: bytes::Bytes,
    bit_pos: usize,
    layout: Layout,
    /// Nesting of generated element loops; see [`MAX_DECODE_DEPTH`](super::MAX_DECODE_DEPTH).
    depth: u16,
}

impl BytesReader {
    /// Owns `data`, positioned at bit 0, MSB-first big-endian.
    #[must_use]
    pub fn new(data: bytes::Bytes) -> Self {
        Self::with_layout(data, Layout::default())
    }

    /// Owns `data` with the given [`Layout`].
    #[must_use]
    pub fn with_layout(data: bytes::Bytes, layout: Layout) -> Self {
        Self {
            data,
            bit_pos: 0,
            layout,
            depth: 0,
        }
    }
}

impl super::sealed::Sealed for BytesReader {}

impl Source for BytesReader {
    #[doc(hidden)]
    fn __bnb_depth(&mut self) -> &mut u16 {
        &mut self.depth
    }
    #[inline]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let mut br = BitReader::with_layout(&self.data, self.layout);
        br.seek_to_bit(self.bit_pos)?;
        let v = br.read_bits(n)?;
        self.bit_pos = Source::bit_pos(&br);
        Ok(v)
    }
    // The slice reader's typed read on the bytes in hand, so a whole-byte value is its
    // one load here too. Forced inline as that read is (DESIGN §22).
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let mut br = BitReader::with_layout(&self.data, self.layout);
        br.seek_to_bit(self.bit_pos)?;
        let value = br.read()?;
        self.bit_pos = Source::bit_pos(&br);
        Ok(value)
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
        self.bit_pos = pos;
        Ok(())
    }
    // An empty run reads nothing, so it must not trip the per-read seek check either.
    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>, BitError> {
        if n == 0 {
            return Ok(Vec::new());
        }
        read_via_slice(&self.data, self.layout, &mut self.bit_pos, |r| {
            r.read_bytes(n)
        })
    }
    fn read_into(&mut self, buf: &mut [u8]) -> Result<(), BitError> {
        if buf.is_empty() {
            return Ok(());
        }
        read_via_slice(&self.data, self.layout, &mut self.bit_pos, |r| {
            r.read_into(buf)
        })
    }
    fn aligned_run(&mut self, n: usize) -> Option<&[u8]> {
        aligned_run_at(&self.data, &mut self.bit_pos, n)
    }
}

impl SeekSource for BytesReader {}

/// A [`Sink`] that encodes into an owned [`BitWriter`]
/// (a `Vec<u8>`, not a `bytes::BytesMut`); [`freeze`] converts that buffer into a
/// `Bytes`.
///
/// [`freeze`]: BytesWriter::freeze
#[derive(Clone, Debug, Default)]
pub struct BytesWriter {
    inner: BitWriter,
}

impl BytesWriter {
    /// An empty MSB-first, big-endian writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty writer in the given [`Layout`].
    #[must_use]
    pub fn with_layout(layout: Layout) -> Self {
        Self {
            inner: BitWriter::with_layout(layout),
        }
    }

    /// The encoded bytes as a zero-copy `Bytes` (the final partial byte is
    /// zero-padded).
    #[must_use]
    pub fn freeze(self) -> bytes::Bytes {
        bytes::Bytes::from(self.inner.into_bytes())
    }
}

impl super::sealed::Sealed for BytesWriter {}

impl Sink for BytesWriter {
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        self.inner.write_bits(value, n)
    }
    // The wrapped writer's typed write, so a whole-byte value is one append here too.
    #[inline]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        Sink::write(&mut self.inner, value)
    }
    fn bit_pos(&self) -> usize {
        Sink::bit_pos(&self.inner)
    }
    fn byte_order(&self) -> ByteOrder {
        Sink::byte_order(&self.inner)
    }
    fn bit_order(&self) -> BitOrder {
        Sink::bit_order(&self.inner)
    }
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        self.inner.write_bytes(bytes)
    }
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        self.inner.aligned_tail(n)
    }
}

/// The bytes one message appends to a `BytesMut`: [`Appendable`] over the buffer's tail
/// from `start`, where the message begins.
#[cfg(feature = "tokio")]
struct Appended<'a> {
    dst: &'a mut bytes::BytesMut,
    start: usize,
}

#[cfg(feature = "tokio")]
impl Appendable for Appended<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.dst.len() - self.start
    }
    #[inline]
    fn push(&mut self, byte: u8) {
        self.dst.extend_from_slice(&[byte]);
    }
    #[inline]
    fn or_into(&mut self, index: usize, bits: u8) {
        self.dst[self.start + index] |= bits;
    }
}

/// A [`Sink`] that appends one message to a `BytesMut` in place, for an encoder that
/// would otherwise build the message in a `Vec` and copy it over. Positions count
/// from the message's first bit, as a [`BitWriter`]'s do, so alignment, padding and
/// error positions do not depend on what the buffer already holds.
///
/// The message stays only if [`keep`](Self::keep) is called. Dropped without it (an
/// encode error, or a panic in the encoder unwinding through), the writer takes back
/// whatever it appended, so the buffer never holds part of a message.
#[cfg(feature = "tokio")]
pub(crate) struct FrameWriter<'a> {
    out: Appended<'a>,
    bit_pos: usize,
    layout: Layout,
    kept: bool,
}

#[cfg(feature = "tokio")]
impl<'a> FrameWriter<'a> {
    /// A writer that appends to `dst` in `layout`, starting at its current end.
    pub(crate) fn new(dst: &'a mut bytes::BytesMut, layout: Layout) -> Self {
        let start = dst.len();
        Self {
            out: Appended { dst, start },
            bit_pos: 0,
            layout,
            kept: false,
        }
    }

    /// Ends the writer leaving the message in the buffer: the encode succeeded.
    pub(crate) fn keep(mut self) {
        self.kept = true;
    }
}

#[cfg(feature = "tokio")]
impl Drop for FrameWriter<'_> {
    fn drop(&mut self) {
        if !self.kept {
            self.out.dst.truncate(self.out.start);
        }
    }
}

#[cfg(feature = "tokio")]
impl super::sealed::Sealed for FrameWriter<'_> {}

#[cfg(feature = "tokio")]
impl Sink for FrameWriter<'_> {
    // As `BitWriter::write_bits`, into the buffer's tail.
    #[inline]
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        let end = self
            .bit_pos
            .checked_add(n)
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        emit_bits(&mut self.out, self.bit_pos, value, n, self.layout.bit);
        self.bit_pos = end;
        Ok(())
    }
    // As `BitWriter::write`: an aligned cursor is the end of what the message wrote.
    #[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
    #[inline(always)]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        if !writes_whole_bytes::<T>(self.bit_pos) {
            return write_by_bits(self, value);
        }
        let Some(end) = self.bit_pos.checked_add(T::BITS as usize) else {
            return Err(position_overflow(self.bit_pos));
        };
        debug_assert_eq!(self.bit_pos / 8, self.out.len());
        let dst = &mut *self.out.dst;
        let byte = self.layout.byte;
        append_whole_bytes(value, byte, |bytes| dst.extend_from_slice(bytes));
        self.bit_pos = end;
        Ok(())
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    #[inline]
    fn bit_order(&self) -> BitOrder {
        self.layout.bit
    }
    // As `BitWriter`: an aligned cursor sits at the end of what the message has
    // written, so the run is appended as it is.
    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        if self.bit_pos % 8 != 0 {
            return write_bytes_per_byte(self, bytes);
        }
        let end = bytes
            .len()
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))
            .ok_or_else(|| BitError::new(ErrorKind::PositionOverflow, self.bit_pos))?;
        self.out.dst.extend_from_slice(bytes);
        self.bit_pos = end;
        Ok(())
    }
    #[inline]
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        if self.bit_pos % 8 != 0 {
            return None;
        }
        let end = n
            .checked_mul(8)
            .and_then(|bits| self.bit_pos.checked_add(bits))?;
        let tail = self.out.dst.len();
        self.out.dst.resize(tail + n, 0);
        self.bit_pos = end;
        Some(&mut self.out.dst[tail..])
    }
}

#[cfg(all(test, feature = "tokio"))]
mod component {
    //! `FrameWriter` against `BitWriter`: the same writes leave the same bytes after
    //! whatever the buffer already held, the same cursor and the same errors.
    use super::FrameWriter;
    use crate::{BitError, BitOrder, BitWriter, ByteOrder, ErrorKind, Layout, Sink};
    use bytes::BytesMut;

    /// A fixed xorshift stream.
    fn noise(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// One write, chosen by `pick`, applied to any sink: bits of a noisy width, a byte
    /// run, a lent tail filled in, or a width no sink accepts.
    fn write<K: Sink>(sink: &mut K, pick: u64, value: u128) -> Result<(), BitError> {
        let bytes = value.to_le_bytes();
        // Reduced first: the noise does not fit a 32-bit `usize`.
        let small = usize::try_from((pick >> 8) % 6).unwrap();
        match pick % 8 {
            0..=3 => sink.write_bits(value, u32::try_from((pick >> 8) % 129).unwrap()),
            4 => sink.write_bits(value, 8 * u32::try_from((pick >> 8) % 17).unwrap()),
            5 => sink.write_bytes(&bytes[..small]),
            6 => {
                if let Some(tail) = sink.aligned_tail(small) {
                    tail.copy_from_slice(&bytes[..small]);
                }
                Ok(())
            }
            _ => sink.write_bits(value, 129 + u32::try_from(pick >> 60).unwrap()),
        }
    }

    #[test]
    fn a_frame_written_in_place_is_the_frame_a_bit_writer_builds() {
        let mut state = 0x94D0_49BB_1331_11EB;
        for round in 0..400 {
            let layout = Layout {
                bit: [BitOrder::Msb, BitOrder::Lsb][round % 2],
                byte: [ByteOrder::Big, ByteOrder::Little][round / 2 % 2],
            };
            // What the buffer held before the frame: it must come through untouched.
            let before: Vec<u8> = (0..round % 4)
                .map(|_| noise(&mut state).to_le_bytes()[0])
                .collect();
            let mut dst = BytesMut::from(&before[..]);
            let mut frame = FrameWriter::new(&mut dst, layout);
            let mut reference = BitWriter::with_layout(layout);
            for step in 0..12 {
                let pick = noise(&mut state);
                let value = u128::from(noise(&mut state)) << 64 | u128::from(noise(&mut state));
                let got = write(&mut frame, pick, value).map_err(|e| (e.kind, e.at));
                let expected = write(&mut reference, pick, value).map_err(|e| (e.kind, e.at));
                assert_eq!(got, expected, "round {round} step {step}");
                assert_eq!(
                    frame.bit_pos(),
                    reference.bit_pos(),
                    "round {round} step {step}"
                );
                assert_eq!(
                    (frame.bit_order(), frame.byte_order()),
                    (layout.bit, layout.byte)
                );
            }
            frame.keep();
            let written = reference.into_bytes();
            assert_eq!(
                dst[..before.len()],
                before[..],
                "round {round}: what was there"
            );
            assert_eq!(dst[before.len()..], written[..], "round {round}: the frame");
        }
    }

    #[test]
    fn a_frame_writer_refuses_a_position_it_cannot_count() {
        let mut dst = BytesMut::from(&b"kept"[..]);
        let mut frame = FrameWriter::new(&mut dst, Layout::default());
        frame.bit_pos = usize::MAX - 3;
        let error = frame.write_bits(0, 8).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 3)
        );
        frame.bit_pos = usize::MAX - 7;
        let error = frame.write_bytes(&[0, 0]).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 7)
        );
        assert!(frame.aligned_tail(2).is_none());
        let error = frame.write(0x0102u16).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 7)
        );
        assert_eq!(frame.bit_pos, usize::MAX - 7);
        frame.keep();
        assert_eq!(&dst[..], b"kept");
    }

    #[test]
    fn a_frame_writer_dropped_without_keep_takes_its_bytes_back() {
        let mut dst = BytesMut::from(&b"kept"[..]);
        let mut frame = FrameWriter::new(&mut dst, Layout::default());
        frame.write_bits(0xABCDE, 20).unwrap();
        frame.write_bytes(&[1, 2, 3]).unwrap();
        drop(frame);
        assert_eq!(&dst[..], b"kept");
    }
}
