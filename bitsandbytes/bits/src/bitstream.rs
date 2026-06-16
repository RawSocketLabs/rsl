//! **Spike:** a bit-level stream codec — read/write fields at arbitrary *bit*
//! offsets, not just byte boundaries.
//!
//! This is the piece `binrw` cannot express: its IO model is a byte
//! `Read + Seek`, so a field that starts mid-byte (a 108-bit DMR payload, a
//! 48-bit sync pattern) forces hand-rolled backward seeks and nibble shifts.
//! [`BitReader`]/[`BitWriter`] track a **bit** cursor over a byte buffer and
//! read/write any [`Bits`] value (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`)
//! directly — bit-aware *and* fast (shift/mask, no `bitvec`).
//!
//! Bit order is big-endian / MSB-first (bit 0 is the high bit of byte 0), which
//! is what RFC/ETSI ASCII-art layouts mean. LSB-first is future work.
//!
//! ```
//! use bits::{u4, u12, BitReader, BitWriter};
//!
//! // Pack a 4-bit then a 12-bit field into a 16-bit (2-byte) stream.
//! let mut w = BitWriter::new();
//! w.write(u4::new(0xA)).unwrap();
//! w.write(u12::new(0xBCD)).unwrap();
//! let bytes = w.into_bytes();
//! assert_eq!(bytes, [0xAB, 0xCD]);
//!
//! let mut r = BitReader::new(&bytes);
//! assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
//! assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD));
//! ```

use core::fmt;

use crate::field::{BitOrder, Bits};

/// A position-aware bit-codec error — the runtime analogue of binrw's error
/// spans. It records the **bit offset** where decoding/encoding failed and, when
/// the derive can supply it, the **field** being processed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitError {
    /// The cause.
    pub kind: ErrorKind,
    /// Absolute bit offset where the error occurred.
    pub at: usize,
    /// The field being decoded/encoded when it occurred, if recorded by the
    /// derive (the innermost field — the "span"). `None` for low-level reader
    /// errors with no field context.
    pub field: Option<&'static str>,
}

/// The cause of a [`BitError`]. Non-exhaustive: later phases add variants
/// (`BadMagic`, …).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Ran past the end of a finite input (a slice): `needed` bits were requested,
    /// `remaining` were left. Definitive — distinct from [`Incomplete`](ErrorKind::Incomplete).
    UnexpectedEof {
        /// Bits requested.
        needed: usize,
        /// Bits still available.
        remaining: usize,
    },
    /// A streaming source ([`StreamBitReader`]) ran out mid-message: the caller
    /// should read more bytes and retry. `needed` is a best-effort byte hint
    /// (`None` when unknown). See [`BitError::is_incomplete`].
    Incomplete {
        /// Best-effort estimate of additional bytes needed, if known.
        needed: Option<usize>,
    },
    /// `decode_exact` left whole bytes unconsumed after the message.
    TrailingBytes {
        /// Number of trailing bytes.
        remaining: usize,
    },
    /// A single field exceeded the 128-bit carrier width.
    TooWide {
        /// The offending width.
        width: usize,
    },
    /// An I/O error while encoding to a [`std::io::Write`] sink.
    Io(std::io::ErrorKind),
}

impl BitError {
    /// Builds an error at absolute bit offset `at`, with no field recorded yet.
    #[must_use]
    pub fn new(kind: ErrorKind, at: usize) -> Self {
        Self {
            kind,
            at,
            field: None,
        }
    }

    /// Records the field being processed, **if one is not already set** — so the
    /// innermost field (set first as the error propagates up) wins. The derive
    /// calls this per field.
    #[must_use]
    pub fn in_field(mut self, field: &'static str) -> Self {
        if self.field.is_none() {
            self.field = Some(field);
        }
        self
    }

    /// Whether this is the streaming "need more bytes" signal
    /// ([`ErrorKind::Incomplete`]) — the caller should read more and retry, as
    /// opposed to a definitive parse failure.
    #[must_use]
    pub fn is_incomplete(&self) -> bool {
        matches!(self.kind, ErrorKind::Incomplete { .. })
    }
}

impl fmt::Display for BitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::UnexpectedEof { needed, remaining } => write!(
                f,
                "unexpected end of input: needed {needed} bits, {remaining} remain"
            )?,
            ErrorKind::Incomplete { needed } => match needed {
                Some(n) => write!(f, "incomplete: need ~{n} more bytes")?,
                None => write!(f, "incomplete: need more bytes")?,
            },
            ErrorKind::TrailingBytes { remaining } => {
                write!(f, "{remaining} trailing bytes after the message")?;
            }
            ErrorKind::TooWide { width } => {
                write!(f, "field width {width} exceeds the 128-bit carrier")?;
            }
            ErrorKind::Io(kind) => write!(f, "I/O error: {kind:?}")?,
        }
        write!(f, " at bit {}", self.at)?;
        if let Some(field) = self.field {
            write!(f, " (field `{field}`)")?;
        }
        Ok(())
    }
}

impl std::error::Error for BitError {}

/// A cursor that reads values at arbitrary bit offsets from a byte slice, in a
/// chosen [`BitOrder`] (MSB-first by default — `bit 0` is the high bit of byte 0,
/// the RFC/ETSI ASCII-art convention; LSB-first for serial/PHY layers).
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
    order: BitOrder,
}

impl<'a> BitReader<'a> {
    /// Wraps `bytes`, positioned at bit 0, **MSB-first**.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::with_order(bytes, BitOrder::Msb)
    }

    /// Wraps `bytes`, positioned at bit 0, in the given bit order.
    #[must_use]
    pub fn with_order(bytes: &'a [u8], order: BitOrder) -> Self {
        Self {
            bytes,
            bit_pos: 0,
            order,
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
        self.bytes.len() * 8 - self.bit_pos
    }

    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, MSB-first.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::UnexpectedEof`] if fewer
    /// than `n` bits remain. Either carries the current bit offset.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        if n > self.remaining_bits() {
            return Err(BitError::new(
                ErrorKind::UnexpectedEof {
                    needed: n,
                    remaining: self.remaining_bits(),
                },
                self.bit_pos,
            ));
        }
        let mut acc: u128 = 0;
        match self.order {
            // MSB-first: first bit read is the field's most-significant.
            BitOrder::Msb => {
                for _ in 0..n {
                    let byte = self.bytes[self.bit_pos >> 3];
                    let bit = (byte >> (7 - (self.bit_pos & 7))) & 1;
                    acc = (acc << 1) | u128::from(bit);
                    self.bit_pos += 1;
                }
            }
            // LSB-first: `bit 0` is a byte's low bit; first bit read is the field's
            // least-significant.
            BitOrder::Lsb => {
                for i in 0..n {
                    let byte = self.bytes[self.bit_pos >> 3];
                    let bit = (byte >> (self.bit_pos & 7)) & 1;
                    acc |= u128::from(bit) << i;
                    self.bit_pos += 1;
                }
            }
        }
        Ok(acc)
    }

    /// Reads one [`Bits`] value of its declared width.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Ok(T::from_bits(self.read_bits(T::BITS)?))
    }

    /// Moves the cursor to absolute bit `pos`. Unlike binrw, this needs no `Seek`
    /// trait and no `NoSeek` wrapper — the whole buffer is in hand, so a seek is
    /// just cursor arithmetic. (Enables e.g. DNS name-compression pointers.)
    ///
    /// # Errors
    /// [`ErrorKind::UnexpectedEof`] if `pos` is past the end of the buffer.
    pub fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        let end = self.bytes.len() * 8;
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
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: usize,
    order: BitOrder,
}

impl BitWriter {
    /// An empty **MSB-first** writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty writer in the given bit order.
    #[must_use]
    pub fn with_order(order: BitOrder) -> Self {
        Self {
            bytes: Vec::new(),
            bit_pos: 0,
            order,
        }
    }

    /// Bits written so far.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.bit_pos
    }

    /// Appends the low `n` (`<= 128`) bits of `value`, in the writer's bit order.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`.
    pub fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        for k in 0..n {
            // MSB-first emits the field's high bit first (i = n-1-k); LSB-first
            // emits its low bit first (i = k) into the byte's low bit.
            let (i, shift) = match self.order {
                BitOrder::Msb => (n - 1 - k, 7 - (self.bit_pos & 7)),
                BitOrder::Lsb => (k, self.bit_pos & 7),
            };
            let byte_idx = self.bit_pos >> 3;
            if byte_idx == self.bytes.len() {
                self.bytes.push(0);
            }
            if (value >> i) & 1 != 0 {
                self.bytes[byte_idx] |= 1 << shift;
            }
            self.bit_pos += 1;
        }
        Ok(())
    }

    /// Appends one [`Bits`] value of its declared width.
    ///
    /// # Errors
    /// As [`write_bits`](Self::write_bits).
    pub fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        self.write_bits(value.into_bits(), T::BITS)
    }

    /// Consumes the writer, returning the packed bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A bit-level **input** the codec recurses over — a [`BitReader`] (in-memory
/// slice) or a [`StreamBitReader`] (forward `Read`). The recursion is generic
/// over `Source`, so one codec runs over any input. (The seekable/streaming
/// `BufSource` ladder is Phase 3.)
pub trait Source {
    /// Reads `n` (`<= 128`) bits MSB-first into the low bits of a `u128`.
    ///
    /// # Errors
    /// Propagates the reader's [`BitError`].
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError>;

    /// The current absolute bit offset (for position-aware errors).
    fn bit_pos(&self) -> usize;

    /// Reads one [`Bits`] value of its declared width.
    ///
    /// # Errors
    /// As [`read_bits`](Source::read_bits).
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Ok(T::from_bits(self.read_bits(T::BITS)?))
    }
}

/// A bit-level **output** the codec writes to — currently the in-memory
/// [`BitWriter`]; a `std::io::Write` adapter is added in chunk B2.
pub trait Sink {
    /// Appends the low `n` (`<= 128`) bits of `value`, MSB-first.
    ///
    /// # Errors
    /// Propagates the writer's [`BitError`].
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError>;

    /// The number of bits written so far.
    fn bit_pos(&self) -> usize;

    /// Appends one [`Bits`] value of its declared width.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        self.write_bits(value.into_bits(), T::BITS)
    }
}

impl Source for BitReader<'_> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        BitReader::read_bits(self, n)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
}

impl Sink for BitWriter {
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        BitWriter::write_bits(self, value, n)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
}

/// A message decoded from a bit stream — the recursion point a
/// `#[derive(BitDecode)]` struct implements (reading each field in declaration
/// order). Leaf fields are any [`Bits`] type; nested messages recurse.
pub trait BitDecode: Sized {
    /// Total bit width of the message — the sum of its fields' widths. The derive
    /// computes it from `<Field as Bits>::BITS`; used to size a byte region when
    /// the message is embedded in a byte stream (see `#[bitwire]`).
    const BIT_LEN: u32;

    /// Decodes `Self` from any [`Source`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the source's [`BitError`].
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError>;
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
pub trait BitEncode {
    /// Encodes `self` into any [`Sink`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError>;
}

// ---------------------------------------------------------------------------
// Entry-point helpers — the logic behind the `#[derive]`-generated inherent
// methods (`Type::decode`/`peek`/`decode_exact`/`encode`/`to_bytes`). Kept here
// so the logic lives in one place rather than monomorphized inline per type;
// doc-hidden because the public surface is the generated methods.
// ---------------------------------------------------------------------------

/// Decodes one message from the front of `buf`, advancing `buf` past the bytes
/// consumed (the tail stays in `buf`). Transactional: on error `buf` is
/// unchanged. Backs `Type::decode`.
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_consume<T: BitDecode>(buf: &mut &[u8], order: BitOrder) -> Result<T, BitError> {
    let input = core::mem::take(buf);
    let mut r = BitReader::with_order(input, order);
    match T::bit_decode(&mut r) {
        Ok(v) => {
            *buf = &input[r.bit_pos().div_ceil(8)..];
            Ok(v)
        }
        Err(e) => {
            *buf = input;
            Err(e)
        }
    }
}

/// Decodes one message from `bytes` without consuming the caller's buffer
/// (tail-tolerant). Backs `Type::peek`.
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_peek<T: BitDecode>(bytes: &[u8], order: BitOrder) -> Result<T, BitError> {
    T::bit_decode(&mut BitReader::with_order(bytes, order))
}

/// Decodes and requires every **whole byte** consumed; a sub-byte tail in the
/// final byte is treated as padding. Backs `Type::decode_exact`.
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the decode error.
#[doc(hidden)]
pub fn decode_exact<T: BitDecode>(bytes: &[u8], order: BitOrder) -> Result<T, BitError> {
    let mut r = BitReader::with_order(bytes, order);
    let v = T::bit_decode(&mut r)?;
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

/// Encodes `value` to a `Vec<u8>`. Backs `Type::to_bytes`.
///
/// # Errors
/// Propagates the encode [`BitError`].
#[doc(hidden)]
pub fn encode_to_vec<T: BitEncode>(value: &T, order: BitOrder) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_order(order);
    value.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// Encodes `value` to any [`std::io::Write`]. Backs `Type::encode`.
///
/// # Errors
/// [`ErrorKind::Io`] on a write failure, else the encode error.
#[doc(hidden)]
pub fn encode_to_writer<T: BitEncode, W: std::io::Write>(
    value: &T,
    w: &mut W,
    order: BitOrder,
) -> Result<(), BitError> {
    let mut bw = BitWriter::with_order(order);
    value.bit_encode(&mut bw)?;
    let at = bw.bit_len();
    w.write_all(&bw.into_bytes())
        .map_err(|e| BitError::new(ErrorKind::Io(e.kind()), at))
}

/// Reads a fixed `[u8; N]` byte array (`N * 8` bits) from the cursor. Backs a
/// `[u8; N]` payload field; `N` is inferred from the field type. Variable-length
/// payloads (`Vec` + `count`) are Phase 2.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn read_byte_array<const N: usize, S: Source>(r: &mut S) -> Result<[u8; N], BitError> {
    let mut arr = [0u8; N];
    for b in &mut arr {
        *b = r.read_bits(8)? as u8;
    }
    Ok(arr)
}

/// Writes a fixed `[u8; N]` byte array. Backs a `[u8; N]` payload field.
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn write_byte_array<const N: usize, K: Sink>(arr: &[u8; N], w: &mut K) -> Result<(), BitError> {
    for &b in arr {
        w.write_bits(u128::from(b), 8)?;
    }
    Ok(())
}

/// **Spike (DESIGN §11 DD3):** a *forward-only* bit reader over any
/// [`std::io::Read`] — the streaming counterpart to the in-memory [`BitReader`].
///
/// It is bounded on `Read` **only, not `Seek`**: binrw's uniform `Read + Seek`
/// requirement (and the `NoSeek` wrapper it forces) is avoided for forward
/// parsing. A seeking variant would add `Read + Seek` *only* where a
/// position-dependent directive needs it — the attribute-driven bound DD3
/// describes. Demonstrated by reading from `&[u8]`, which is `Read` but **not**
/// `Seek`. Reads up to 64 bits per call.
#[derive(Debug)]
pub struct StreamBitReader<R> {
    inner: R,
    /// Buffered-but-unconsumed bits, right-aligned in the low `nbits` bits.
    acc: u128,
    nbits: u32,
    /// Total bits consumed so far (for position-aware errors).
    pos: usize,
}

impl<R: std::io::Read> StreamBitReader<R> {
    /// Wraps a byte source.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            acc: 0,
            nbits: 0,
            pos: 0,
        }
    }

    /// The total number of bits consumed so far.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    /// Reads `n` (`<= 64`) bits MSB-first, pulling bytes from the source as needed.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 64`; [`ErrorKind::Incomplete`] if the
    /// source runs out mid-field (read more and retry). Either carries the bit
    /// offset.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 64 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.pos,
            ));
        }
        let at = self.pos;
        while self.nbits < n {
            let mut b = [0u8; 1];
            if self.inner.read_exact(&mut b).is_err() {
                // A stream ran out mid-field: signal "need more bytes" (the caller
                // can buffer and retry), not a definitive end-of-input.
                return Err(BitError::new(ErrorKind::Incomplete { needed: None }, at));
            }
            self.acc = (self.acc << 8) | u128::from(b[0]);
            self.nbits += 8;
        }
        let shift = self.nbits - n;
        let take = if n == 0 { 0 } else { (1u128 << n) - 1 };
        let val = (self.acc >> shift) & take;
        self.nbits = shift;
        let keep = if shift == 0 { 0 } else { (1u128 << shift) - 1 };
        self.acc &= keep;
        self.pos += n as usize;
        Ok(val)
    }

    /// Reads one [`Bits`] value (width `<= 64`) of its declared width.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Ok(T::from_bits(self.read_bits(T::BITS)?))
    }
}

impl<R: std::io::Read> Source for StreamBitReader<R> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        StreamBitReader::read_bits(self, n)
    }
    fn bit_pos(&self) -> usize {
        self.pos
    }
}

/// binrw bridge: `parse_with`/`write_with` helpers that embed a bit-decoded
/// region inside a `#[binrw]`/`#[bitwire]` struct. This is the **dispatch seam**
/// — binrw owns the byte-aligned stream (magic/count/args/…), these hand a
/// byte-aligned sub-region to the bit cursor. Used by the `#[bitwire]` macro
/// (DESIGN §11 DD1).
#[cfg(feature = "binrw")]
mod binrw_bridge {
    use super::{BitDecode, BitEncode, BitError, BitReader, BitWriter};
    use binrw::io::{Read, Seek, Write};
    use binrw::{BinResult, Endian};

    /// `parse_with` bridge: read `T`'s byte-region from the stream, bit-decode it.
    ///
    /// # Errors
    /// I/O errors from the reader, or a [`BitError`] wrapped as `binrw::Error::Custom`.
    pub fn read_bits_region<T, R>(reader: &mut R, _endian: Endian, _args: ()) -> BinResult<T>
    where
        T: BitDecode,
        R: Read + Seek,
    {
        let pos = reader.stream_position()?;
        let n = (T::BIT_LEN as usize).div_ceil(8);
        let mut buf = vec![0u8; n];
        reader.read_exact(&mut buf)?;
        let mut br = BitReader::new(&buf);
        T::bit_decode(&mut br).map_err(|e: BitError| binrw::Error::Custom {
            pos,
            err: Box::new(e),
        })
    }

    /// `write_with` bridge: bit-encode `T`, emit its bytes.
    ///
    /// # Errors
    /// I/O errors from the writer, or a [`BitError`] wrapped as `binrw::Error::Custom`.
    pub fn write_bits_region<T, W>(
        value: &T,
        writer: &mut W,
        _endian: Endian,
        _args: (),
    ) -> BinResult<()>
    where
        T: BitEncode,
        W: Write + Seek,
    {
        let pos = writer.stream_position()?;
        let mut bw = BitWriter::new();
        value
            .bit_encode(&mut bw)
            .map_err(|e: BitError| binrw::Error::Custom {
                pos,
                err: Box::new(e),
            })?;
        writer.write_all(&bw.into_bytes())?;
        Ok(())
    }
}

#[cfg(feature = "binrw")]
pub use binrw_bridge::{read_bits_region, write_bits_region};

#[cfg(test)]
mod unit {
    use super::*;
    use crate::{u4, u12};

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
}
