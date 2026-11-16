//! The write side of the cursor abstraction: [`Sink`] and the `Write` view.

use core::any::Any;

use crate::field::{BitOrder, Bits, ByteOrder};

#[cfg(all(doc, feature = "std"))]
use super::SourceReader;
use super::layout::value_byte_order;
use super::{BitError, ErrorKind, sealed};
#[cfg(doc)]
use super::{BitWriter, Source};

/// A bit-level **output** the codec writes to — the in-memory [`BitWriter`]
/// (and, under the `bytes` feature, `BytesWriter`). Encode to any
/// `std::io::Write` via the `EncodeExt::encode` extension method (with `std`).
///
/// # Examples
///
/// ```
/// use bnb::{BitWriter, Sink, u4};
///
/// // A writer generic over any `Sink`.
/// fn put_nibble<K: Sink>(k: &mut K, v: u4) { k.write(v).unwrap(); }
///
/// let mut w = BitWriter::new();
/// put_nibble(&mut w, u4::new(0xA));
/// put_nibble(&mut w, u4::new(0x5));
/// assert_eq!(w.into_bytes(), [0xA5]);
/// ```
pub trait Sink: sealed::Sealed {
    /// Appends the low `n` (`<= 128`) bits of `value`, in the sink's bit order
    /// (MSB-first by default).
    ///
    /// # Errors
    /// Propagates the writer's [`BitError`].
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError>;

    /// The number of bits written so far.
    fn bit_pos(&self) -> usize;

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
    }

    /// The bit order this sink writes in (default MSB-first). Paired with
    /// [`byte_order`](Sink::byte_order) exactly as on [`Source`]: each bit order has a
    /// *natural* byte layout (big-endian under MSB, little-endian under LSB), and only the
    /// opposite declaration swaps a byte-multiple value.
    fn bit_order(&self) -> BitOrder {
        BitOrder::Msb
    }

    /// A type-erased, **encode-scoped scratch** value for codecs that need mutable state
    /// shared across a whole message's fields — a back-reference / compression dictionary
    /// (e.g. DNS name compression). Recover the concrete type with
    /// `Any::downcast_mut`.
    ///
    /// Returns `None` unless the sink was built carrying one (see
    /// [`BitWriter::with_scratch`]); the default sink has none. The scratch is the shared
    /// thread the `Sink` already provides — since one sink is passed by `&mut` through
    /// every field's encode, a value stored here is visible to them all.
    fn scratch(&mut self) -> Option<&mut dyn Any> {
        None
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    #[inline]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        write_by_bits(self, value)
    }

    /// Appends a run of bytes — the bulk dual of [`Source::read_bytes`], replacing the
    /// per-byte `write(b)?` loop in custom codecs. Works at any bit offset. [`BitWriter`]
    /// (and `BytesWriter`) append a byte-aligned run in one copy and otherwise run the same
    /// per-byte loop as this default.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), BitError> {
        write_bytes_per_byte(self, bytes)
    }

    /// Appends `n` zero bytes and lends them for filling, when the cursor is byte-aligned
    /// and the longer output's bits can still be counted; otherwise `None`, writing nothing.
    /// The hook behind bulk counted runs of byte-multiple integers
    /// ([`BitEncode::encode_slice`]): a caller that gets `None` writes the run element by
    /// element, so every error is that path's. The default lends nothing; the sinks that
    /// bulk-copy [`write_bytes`](Sink::write_bytes) override it.
    #[doc(hidden)]
    fn aligned_tail(&mut self, n: usize) -> Option<&mut [u8]> {
        let _ = n;
        None
    }

    /// Borrows this sink as a [`std::io::Write`] — the dual of [`Source::as_read`], for
    /// handing the cursor to `std::io`-based code from a `#[bw(write_with = …)]`. Writes 8
    /// bits per byte; see [`SinkWriter`]. Only with the `std` feature.
    #[cfg(feature = "std")]
    fn as_write(&mut self) -> SinkWriter<'_, Self>
    where
        Self: Sized,
    {
        SinkWriter(self)
    }
}

/// A [`std::io::Write`] view over a [`Sink`], from [`Sink::as_write`]. Each `write`
/// pushes 8 bits per byte through [`Sink::write_bits`]. The outbound dual of
/// [`SourceReader`]; `flush` is a no-op (the sink owns its buffer).
#[cfg(feature = "std")]
pub struct SinkWriter<'a, K: Sink>(&'a mut K);

#[cfg(feature = "std")]
impl<K: Sink> std::io::Write for SinkWriter<'_, K> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            self.0
                .write_bits(u128::from(b), 8)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// [`Sink::write`] for any sink and width: the byte order, then the bits.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub(super) fn write_by_bits<K: Sink + ?Sized, T: Bits>(
    k: &mut K,
    value: T,
) -> Result<(), BitError> {
    let raw = value_byte_order::<T>(value.into_bits(), k.bit_order(), k.byte_order());
    k.write_bits(raw, T::BITS)
}

/// Whether a `T` written at `bit_pos` is whole bytes from a byte-aligned cursor, the case
/// [`append_whole_bytes`] serves.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub(super) const fn writes_whole_bytes<T: Bits>(bit_pos: usize) -> bool {
    T::BITS % 8 == 0 && T::BITS <= 128 && bit_pos % 8 == 0
}

/// Hands `append` the bytes a whole-byte `value` puts on the wire. Aligned whole bytes
/// are the value big-endian or little-endian by the effective byte order alone, whatever
/// the bit order (the mirror of `BitReader::take_whole_bytes`), so an appending writer
/// stores them in one step.
#[expect(clippy::inline_always, reason = "measured: DESIGN §22")]
#[inline(always)]
pub(super) fn append_whole_bytes<T: Bits>(value: T, byte: ByteOrder, append: impl FnOnce(&[u8])) {
    debug_assert!(T::BITS % 8 == 0 && T::BITS <= 128);
    let n = (T::BITS / 8) as usize;
    let raw = value.into_bits();
    match T::__BNB_BYTE_ORDER.unwrap_or(byte) {
        ByteOrder::Big => append(&raw.to_be_bytes()[16 - n..]),
        ByteOrder::Little => append(&raw.to_le_bytes()[..n]),
    }
}

/// The error for a write that would move a writer at `bit_pos` past what `usize` counts.
#[cold]
#[inline(never)]
pub(super) fn position_overflow(bit_pos: usize) -> BitError {
    BitError::new(ErrorKind::PositionOverflow, bit_pos)
}

pub(super) fn write_bytes_per_byte<K: Sink + ?Sized>(
    k: &mut K,
    bytes: &[u8],
) -> Result<(), BitError> {
    for &b in bytes {
        k.write(b)?;
    }
    Ok(())
}

#[cfg(test)]
mod unit {
    #[cfg(feature = "bytes")]
    use crate::bitstream::BytesWriter;
    #[cfg(feature = "tokio")]
    use crate::bitstream::FrameWriter;
    use crate::bitstream::fixtures::{Blank, Own, noise};
    use crate::bitstream::{BitEncode, BitWriter, ErrorKind, Layout};
    use crate::{u4, u12};

    use super::*;

    /// A 16-bit value whose `into_bits` leaves bits set above its width, which no write
    /// may put on the wire.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Dirty(u16);

    impl Bits for Dirty {
        const BITS: u32 = 16;

        fn into_bits(self) -> u128 {
            u128::from(self.0) | (0xA5A5 << 16) | (0xFF << 120)
        }

        fn from_bits(raw: u128) -> Self {
            Self(u16::try_from(raw).expect("a 16-bit value"))
        }
    }

    /// What a writer holds before the write under test: `lead` bits, then by `kind`
    /// nothing more, a byte run, a typed write, or (from a byte boundary) a lent tail
    /// filled in. Each leaves the cursor and the buffer in step in its own way.
    fn write_prefix<K: Sink>(sink: &mut K, lead: u32, kind: u8) {
        sink.write_bits(0x1_5555, lead).unwrap();
        match kind {
            1 => sink.write_bytes(&[0x11, 0x22, 0x33]).unwrap(),
            2 => sink.write(0xBEEFu16).unwrap(),
            3 => {
                if let Some(tail) = sink.aligned_tail(2) {
                    tail.copy_from_slice(&[0x44, 0x55]);
                }
            }
            _ => {}
        }
    }

    /// One `T` written after `lead` bits, by each appending writer's `write`, against the
    /// same write taken bit by bit ([`write_by_bits`], the path every other sink uses):
    /// the result, the cursor and the bytes.
    fn assert_write_matches_the_bit_path<T: Bits>(
        layout: Layout,
        (lead, kind): (u32, u8),
        raw: u128,
    ) {
        type Outcome = (Result<(), (ErrorKind, usize)>, usize, Vec<u8>);
        fn flat(result: Result<(), BitError>) -> Result<(), (ErrorKind, usize)> {
            result.map_err(|e| (e.kind, e.at))
        }
        let mask = match T::BITS {
            0 => 0,
            bits if bits >= 128 => u128::MAX,
            bits => (1 << bits) - 1,
        };
        let value = || T::from_bits(raw & mask);
        let context = format!("{layout:?} lead {lead} prefix {kind} width {}", T::BITS);
        let start = || {
            let mut writer = BitWriter::with_layout(layout);
            write_prefix(&mut writer, lead, kind);
            // A clone starts a session of its own and must write as the original does.
            writer.clone()
        };
        let done = |result, writer: BitWriter| -> Outcome {
            (flat(result), writer.bit_pos, writer.into_bytes())
        };

        let mut slow = start();
        let result = write_by_bits(&mut slow, value());
        let expected = done(result, slow);
        let mut fast = start();
        let result = Sink::write(&mut fast, value());
        assert_eq!(done(result, fast), expected, "{context}");
        let mut inherent = start();
        let result = BitWriter::write(&mut inherent, value());
        assert_eq!(done(result, inherent), expected, "{context}, inherent");

        #[cfg(feature = "bytes")]
        {
            let mut writer = BytesWriter::with_layout(layout);
            write_prefix(&mut writer, lead, kind);
            let result = flat(writer.write(value()));
            let got = (result, writer.bit_pos(), writer.freeze().to_vec());
            assert_eq!(got, expected, "{context}, bytes");
        }
        #[cfg(feature = "tokio")]
        {
            let mut dst = bytes::BytesMut::from(&b"kept"[..]);
            let mut frame = FrameWriter::new(&mut dst, layout);
            write_prefix(&mut frame, lead, kind);
            let result = flat(frame.write(value()));
            let end = frame.bit_pos();
            frame.keep();
            assert_eq!(&dst[..4], b"kept", "{context}, frame");
            assert_eq!(
                (result, end, dst[4..].to_vec()),
                expected,
                "{context}, frame"
            );
        }
    }

    #[test]
    fn write_appends_whole_bytes_exactly_as_the_bit_path_does() {
        use crate::{u24, u40, u48, u56, u72, u80, u88, u96, u104, u112, u120};
        let mut state = 0xD6E8_FEB8_6659_FD93;
        for bit in [BitOrder::Msb, BitOrder::Lsb] {
            for byte in [ByteOrder::Big, ByteOrder::Little] {
                let layout = Layout { bit, byte };
                // From a byte boundary and from every offset inside a byte, after zero,
                // one and two whole bytes, behind each kind of earlier write.
                for lead in 0..=17 {
                    for kind in 0..4 {
                        let raw =
                            u128::from(noise(&mut state)) << 64 | u128::from(noise(&mut state));
                        let before = (lead, kind);
                        macro_rules! each {
                            ($($value:ty),*) => {$(
                                assert_write_matches_the_bit_path::<$value>(layout, before, raw);
                            )*};
                        }
                        each!(bool, u4, u12, u8, u16, u24, u32, u40, u48, u56, u64, u72);
                        each!(u80, u88, u96, u104, u112, u120, u128);
                        each!(Own<true>, Own<false>, Dirty, Blank<0>, Blank<136>);
                    }
                }
            }
        }
    }

    #[test]
    fn a_typed_write_refuses_a_position_it_cannot_count() {
        let mut w = BitWriter::new();
        // Byte-aligned, eight bits from the bound: a sixteen-bit value does not fit.
        w.bit_pos = usize::MAX - 7;
        let error = w.write(0x0102u16).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 7)
        );
        // A width no write accepts is refused for that first, as by the bit path.
        let error = w.write(Blank::<136>).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::TooWide { width: 136 }, usize::MAX - 7)
        );
        // From inside a byte the bit path reports the same bound.
        w.bit_pos = usize::MAX - 3;
        let error = w.write(0u8).unwrap_err();
        assert_eq!(
            (error.kind, error.at),
            (ErrorKind::PositionOverflow, usize::MAX - 3)
        );
        assert_eq!(w.bit_pos, usize::MAX - 3);
        assert_eq!(w.bytes, []);
    }

    /// A `Sink` that overrides only the required methods, exercising the default
    /// `byte_order`/`write`.
    struct TinySink {
        out: Vec<u8>,
        bit: usize,
    }
    impl sealed::Sealed for TinySink {}

    impl Sink for TinySink {
        fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
            let n = n as usize;
            for k in 0..n {
                let p = self.bit + k;
                if p >> 3 == self.out.len() {
                    self.out.push(0);
                }
                if (value >> (n - 1 - k)) & 1 != 0 {
                    self.out[p >> 3] |= 1 << (7 - (p & 7));
                }
            }
            self.bit += n;
            Ok(())
        }
        fn bit_pos(&self) -> usize {
            self.bit
        }
    }

    #[test]
    fn sink_default_byte_order_is_big() {
        let s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        assert_eq!(s.byte_order(), ByteOrder::Big);
    }

    #[test]
    fn sink_default_write_dispatches_through_write_bits() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        s.write(0xABu8).unwrap();
        s.write(0xCDu8).unwrap();
        assert_eq!(s.out, [0xAB, 0xCD]);
    }

    #[test]
    fn sink_default_byte_runs_write_per_byte() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        s.write_bits(0xF, 4).unwrap();
        s.write_bytes(&[0xAB, 0xCD]).unwrap();
        assert_eq!(s.out, [0xFA, 0xBC, 0xD0]);
        assert_eq!(s.bit, 20);
    }

    #[test]
    fn sink_default_lends_no_tail_so_an_integer_run_is_written_each() {
        let mut s = TinySink {
            out: Vec::new(),
            bit: 0,
        };
        assert!(s.aligned_tail(4).is_none());
        u16::encode_slice(&[0x0102, 0x0304], &mut s).unwrap();
        assert_eq!(s.out, [1, 2, 3, 4]);
        assert_eq!(s.bit, 32);
    }
}
