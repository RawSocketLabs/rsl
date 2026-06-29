//! A bit-level stream codec — read/write fields at arbitrary *bit* offsets, not
//! just byte boundaries.
//!
//! A byte-oriented `Read + Seek` codec can only address byte boundaries, so a field
//! that starts mid-byte (a 108-bit DMR payload, a 48-bit sync pattern) forces
//! hand-rolled backward seeks and nibble shifts.
//! [`BitReader`]/[`BitWriter`] track a **bit** cursor over a byte buffer and
//! read/write any [`Bits`] value (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`)
//! directly — bit-aware *and* fast (shift/mask, no `bitvec`).
//!
//! The wire [`Layout`] is configurable: bit order (MSB-first default — bit 0 is the
//! high bit of byte 0, the RFC/ETSI convention — or LSB-first) and byte order (big-
//! endian default, or little-endian for byte-multiple values).
//!
//! ```
//! use bnb::{u4, u12, BitReader, BitWriter};
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

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::field::{BitOrder, Bits, ByteOrder};

/// A position-aware bit-codec error (it carries a span-like position). It records the
/// **bit offset** where decoding/encoding failed and, when the derive can supply it,
/// the **field** being processed.
///
/// # Examples
///
/// ```
/// use bnb::{bin, ErrorKind};
///
/// #[bin(big)]
/// #[derive(Debug)]
/// struct Pair { a: u16, b: u16 }
///
/// let err = Pair::decode_exact(&[0x00]).unwrap_err(); // only one byte of four
/// assert_eq!(err.at, 0);             // the bit offset where it failed
/// assert_eq!(err.field, Some("a"));  // the field being read (the span)
/// assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
/// ```
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
    /// An I/O error while encoding to a [`std::io::Write`] sink (the `std` feature).
    #[cfg(feature = "std")]
    Io(std::io::ErrorKind),
    /// A `magic` constant read off the wire did not match. Both values are the
    /// type-erased low-bit representations ([`Bits::into_bits`]).
    BadMagic {
        /// The constant the codec expected.
        expected: u128,
        /// The value actually read.
        found: u128,
    },
    /// A `try_map` conversion from the wire representation failed; `message` is the
    /// converter's `Display` output.
    Convert {
        /// The converter's error, rendered.
        message: String,
    },
    /// A position directive (`restore_position`/seek) ran on a non-seekable
    /// [`Source`] (a forward-only stream). Decode from a slice ([`BitReader`]) or a
    /// seekable source instead.
    NotSeekable,
    /// A [`BufSource`] hit its retention cap before the message finished — the
    /// framed message is larger than the configured bound (never unbounded).
    BufferFull {
        /// The cap, in bytes.
        cap: usize,
    },
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

    /// Builds a [`ErrorKind::BadMagic`] error (a `magic` constant mismatched) at
    /// absolute bit offset `at`. `expected`/`found` are the type-erased low-bit
    /// values ([`Bits::into_bits`]).
    #[must_use]
    pub fn bad_magic(expected: u128, found: u128, at: usize) -> Self {
        Self::new(ErrorKind::BadMagic { expected, found }, at)
    }

    /// Builds a [`ErrorKind::Convert`] error (a `try_map` conversion failed) at
    /// absolute bit offset `at`.
    #[must_use]
    pub fn convert(message: String, at: usize) -> Self {
        Self::new(ErrorKind::Convert { message }, at)
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

#[cfg(feature = "std")]
impl From<std::io::Error> for BitError {
    /// Wraps a [`std::io::Error`] as [`ErrorKind::Io`] — so a `parse_with`/`write_with`
    /// using [`Source::as_read`]/[`Sink::as_write`] can `?` `std::io` results straight
    /// into a `BitError`. The bit offset is unknown at this boundary (recorded as `0`);
    /// build with [`BitError::new`] if you need the precise position.
    fn from(e: std::io::Error) -> Self {
        BitError::new(ErrorKind::Io(e.kind()), 0)
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
            #[cfg(feature = "std")]
            ErrorKind::Io(kind) => write!(f, "I/O error: {kind:?}")?,
            ErrorKind::BadMagic { expected, found } => {
                write!(f, "bad magic: expected {expected:#x}, found {found:#x}")?;
            }
            ErrorKind::Convert { message } => {
                write!(f, "conversion failed: {message}")?;
            }
            ErrorKind::NotSeekable => {
                write!(f, "a position directive ran on a non-seekable source")?;
            }
            ErrorKind::BufferFull { cap } => {
                write!(f, "buffered source exceeded its {cap}-byte cap")?;
            }
        }
        write!(f, " at bit {}", self.at)?;
        if let Some(field) = self.field {
            write!(f, " (field `{field}`)")?;
        }
        Ok(())
    }
}

impl core::error::Error for BitError {}

impl From<crate::error::Error> for BitError {
    /// Bridges a construction error (e.g. `UInt::try_new`) into a codec error, so it
    /// `?`-propagates inside a custom `parse_with`/`write_with` fn or a converter
    /// that returns [`BitError`]. The offset is unknown (`0`) — the codec's own
    /// reads/writes carry the real bit offset; this is only for borrowed construction
    /// failures with no cursor context.
    #[inline]
    fn from(e: crate::error::Error) -> Self {
        BitError::convert(e.to_string(), 0)
    }
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

/// A typed bit/byte amount for positioning directives — `4.bits()`, `3.bytes()` —
/// resolving to a bit count. Bring it in with `use bnb::prelude::*`.
///
/// # Examples
///
/// ```
/// use bnb::prelude::*;
/// assert_eq!(4u32.bits(), 4);
/// assert_eq!(3u32.bytes(), 24);
/// ```
///
/// Used by the positioning directives, e.g. `#[br(pad_before = 2u32.bytes())]` — see
/// [`guide::directives`](crate::guide::directives).
pub trait BitAmount: Copy {
    /// This many **bits**.
    fn bits(self) -> u32;
    /// This many **bytes** (× 8 bits).
    fn bytes(self) -> u32;
}

macro_rules! impl_bit_amount {
    ($($t:ty),*) => {$(
        impl BitAmount for $t {
            fn bits(self) -> u32 { self as u32 }
            fn bytes(self) -> u32 { (self as u32) * 8 }
        }
    )*};
}
impl_bit_amount!(u8, u16, u32, u64, usize, i32);

/// Skips `bits` forward (consuming and discarding) — backs `#[br(pad_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn skip_read<S: Source>(r: &mut S, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        r.read_bits(n)?;
        left -= n;
    }
    Ok(())
}

/// Writes `bits` zero bits forward — the write dual of [`skip_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn skip_write<K: Sink>(w: &mut K, bits: u32) -> Result<(), BitError> {
    let mut left = bits;
    while left > 0 {
        let n = left.min(128);
        w.write_bits(0, n)?;
        left -= n;
    }
    Ok(())
}

/// Skips forward to the next byte boundary — backs `#[br(align_before/after)]`.
///
/// # Errors
/// Propagates the source's [`BitError`].
#[doc(hidden)]
pub fn align_read<S: Source>(r: &mut S) -> Result<(), BitError> {
    let pad = (8 - (r.bit_pos() % 8)) % 8;
    skip_read(r, pad as u32)
}

/// Pads with zero bits to the next byte boundary — the write dual of [`align_read`].
///
/// # Errors
/// Propagates the sink's [`BitError`].
#[doc(hidden)]
pub fn align_write<K: Sink>(w: &mut K) -> Result<(), BitError> {
    let pad = (8 - (w.bit_pos() % 8)) % 8;
    skip_write(w, pad as u32)
}

/// The wire layout: bit packing order **and** byte order, threaded through the
/// cursors and entry points. `#[bin(big|little)]` and `#[bin(bit_order = msb|lsb)]`
/// set it; the default is MSB-first, big-endian (RFC/network order).
///
/// # Examples
///
/// ```
/// use bnb::{BitReader, BitOrder, ByteOrder, Layout};
///
/// // Read a 16-bit value little-endian instead of the default big-endian.
/// let layout = Layout { bit: BitOrder::Msb, byte: ByteOrder::Little };
/// let mut r = BitReader::with_layout(&[0x34, 0x12], layout);
/// assert_eq!(r.read::<u16>().unwrap(), 0x1234);
/// assert_eq!(Layout::default(), Layout { bit: BitOrder::Msb, byte: ByteOrder::Big });
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Bit packing order — does the first bit land in the high or low bit.
    pub bit: BitOrder,
    /// Byte order, applied to byte-multiple values.
    pub byte: ByteOrder,
}

/// Reverses the low `bits / 8` bytes of `raw` when little-endian and the width is a
/// whole number of bytes (byte order applies only to byte-multiple values); a
/// no-op for big-endian or sub-byte widths. It is its own inverse, so read and
/// write share it.
#[inline]
fn apply_byte_order(raw: u128, bits: u32, byte: ByteOrder) -> u128 {
    if byte == ByteOrder::Big || bits % 8 != 0 {
        return raw;
    }
    let n = (bits / 8) as usize;
    let le = raw.to_le_bytes();
    let mut out = 0u128;
    let mut i = 0;
    while i < n {
        out |= (le[i] as u128) << (8 * (n - 1 - i));
        i += 1;
    }
    out
}

/// Extracts `n` (`<= 128`) bits starting at absolute bit offset `pos` from `buf`, in
/// `order`, returned right-aligned in a `u128` (byte order is applied separately by
/// `read`). The single bit-extraction routine behind every slice-backed [`Source`]
/// ([`BitReader`], [`BufSource`], [`SeekReader`]). The caller must have bounds-checked
/// `pos + n <= buf.len() * 8` and `n <= 128`.
///
/// **Fast path:** when the read is byte-aligned (`pos % 8 == 0` and `n % 8 == 0`) the
/// bytes are accumulated whole — one iteration per byte, not per bit (≈8× fewer).
#[inline]
fn extract_bits(buf: &[u8], pos: usize, n: usize, order: BitOrder) -> u128 {
    if pos % 8 == 0 && n % 8 == 0 {
        let start = pos / 8;
        let nbytes = n / 8;
        let mut acc = 0u128;
        match order {
            // MSB-first byte-aligned == big-endian byte concatenation.
            BitOrder::Msb => {
                for j in 0..nbytes {
                    acc = (acc << 8) | u128::from(buf[start + j]);
                }
            }
            // LSB-first byte-aligned == little-endian byte concatenation.
            BitOrder::Lsb => {
                for j in 0..nbytes {
                    acc |= u128::from(buf[start + j]) << (8 * j);
                }
            }
        }
        return acc;
    }
    // General path: one bit at a time (handles sub-byte offsets/widths).
    let mut acc = 0u128;
    match order {
        BitOrder::Msb => {
            for k in 0..n {
                let p = pos + k;
                acc = (acc << 1) | u128::from((buf[p >> 3] >> (7 - (p & 7))) & 1);
            }
        }
        BitOrder::Lsb => {
            for k in 0..n {
                let p = pos + k;
                acc |= u128::from((buf[p >> 3] >> (p & 7)) & 1) << k;
            }
        }
    }
    acc
}

/// Appends the low `n` (`<= 128`) bits of `value` to `out` at absolute bit offset
/// `bit_pos`, in `order` — the write dual of [`extract_bits`], used by [`BitWriter`].
///
/// **Fast path:** when appending byte-aligned at the end (`bit_pos % 8 == 0`,
/// `n % 8 == 0`, cursor at `out.len()`) the bytes are pushed whole, one per byte.
#[inline]
fn emit_bits(out: &mut Vec<u8>, bit_pos: usize, value: u128, n: usize, order: BitOrder) {
    if n % 8 == 0 && bit_pos % 8 == 0 && bit_pos / 8 == out.len() {
        let nbytes = n / 8;
        match order {
            BitOrder::Msb => {
                for j in 0..nbytes {
                    out.push((value >> (8 * (nbytes - 1 - j))) as u8);
                }
            }
            BitOrder::Lsb => {
                for j in 0..nbytes {
                    out.push((value >> (8 * j)) as u8);
                }
            }
        }
        return;
    }
    for k in 0..n {
        let p = bit_pos + k;
        // MSB-first emits the field's high bit first (i = n-1-k); LSB-first emits its
        // low bit first (i = k) into the byte's low bit.
        let (i, shift) = match order {
            BitOrder::Msb => (n - 1 - k, 7 - (p & 7)),
            BitOrder::Lsb => (k, p & 7),
        };
        let byte_idx = p >> 3;
        if byte_idx == out.len() {
            out.push(0);
        }
        if (value >> i) & 1 != 0 {
            out[byte_idx] |= 1 << shift;
        }
    }
}

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
    bytes: &'a [u8],
    bit_pos: usize,
    order: BitOrder,
    byte: ByteOrder,
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

    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the reader's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::UnexpectedEof`] if fewer
    /// than `n` bits remain. Either carries the current bit offset.
    #[inline]
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
        let acc = extract_bits(self.bytes, self.bit_pos, n, self.order);
        self.bit_pos += n;
        Ok(acc)
    }

    /// Reads one [`Bits`] value of its declared width, applying the byte order to a
    /// byte-multiple value.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    #[inline]
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(apply_byte_order(raw, T::BITS, self.byte)))
    }

    /// Moves the cursor to absolute bit `pos`. This needs no `Seek` trait — the whole
    /// buffer is in hand, so a seek is just cursor arithmetic. (Enables e.g. DNS
    /// name-compression pointers.)
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
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: usize,
    order: BitOrder,
    byte: ByteOrder,
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
    #[inline]
    pub fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::new(ErrorKind::TooWide { width: n }, self.bit_pos));
        }
        emit_bits(&mut self.bytes, self.bit_pos, value, n, self.order);
        self.bit_pos += n;
        Ok(())
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order to
    /// a byte-multiple value.
    ///
    /// # Errors
    /// As [`write_bits`](Self::write_bits).
    #[inline]
    pub fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        let raw = apply_byte_order(value.into_bits(), T::BITS, self.byte);
        self.write_bits(raw, T::BITS)
    }

    /// Consumes the writer, returning the packed bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A bit-level **input** the codec recurses over. Implemented by [`BitReader`]
/// (in-memory slice), [`StreamBitReader`] (forward `Read`), [`BufSource`] (a
/// retain-and-seek socket adapter), and [`SeekReader`] (`Read + Seek`); the codec is
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
pub trait Source {
    /// Reads `n` (`<= 128`) bits into the low bits of a `u128`, in the source's
    /// bit order (MSB-first by default).
    ///
    /// # Errors
    /// Propagates the reader's [`BitError`].
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError>;

    /// The current absolute bit offset (for position-aware errors).
    fn bit_pos(&self) -> usize;

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
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
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(apply_byte_order(
            raw,
            T::BITS,
            self.byte_order(),
        )))
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
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for (i, slot) in buf.iter_mut().enumerate() {
            match self.0.read_bits(8) {
                Ok(b) => *slot = b as u8,
                Err(e) if i == 0 => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        e.to_string(),
                    ));
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
/// compile time. Implemented by [`BitReader`], [`BufSource`], and [`SeekReader`]
/// (and, with the `bytes` feature, `BytesReader`).
pub trait SeekSource: Source {}

impl SeekSource for BitReader<'_> {}

/// A bit-level **output** the codec writes to — the in-memory [`BitWriter`]
/// (and, under the `bytes` feature, `BytesWriter`). Encode to any
/// [`std::io::Write`] via a message's generated `encode` method.
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
pub trait Sink {
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

    /// Appends one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    #[inline]
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        let raw = apply_byte_order(value.into_bits(), T::BITS, self.byte_order());
        self.write_bits(raw, T::BITS)
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

impl Source for BitReader<'_> {
    #[inline]
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        BitReader::read_bits(self, n)
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
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        BitReader::seek_to_bit(self, pos)
    }
}

impl Sink for BitWriter {
    #[inline]
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        BitWriter::write_bits(self, value, n)
    }
    #[inline]
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    #[inline]
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
}

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
}

/// Which form [`EncodeExt::encode`] writes. On a `#[bin]` message that has a `reserved` or
/// `calc` field, this is a settable in-memory property (`encode_mode()`/`set_encode_mode()`,
/// or the builder's `.encode_mode(…)`) that `encode` consults — never written to the wire.
/// `to_bytes`/`to_canonical_bytes` ignore it and always encode verbatim/canonical respectively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EncodeMode {
    /// **Verbatim** *(default)* — write exactly what's stored (retained `reserved` bits, the
    /// stored value of a `calc` field). Never silently rewrites the caller's data, and is the
    /// faithful dual of `decode` (so a decoded value re-encodes byte-for-byte).
    #[default]
    Verbatim,
    /// **Canonical** — `reserved` fields written as their spec value and `calc` fields
    /// recomputed, so the result is always spec-compliant.
    Canonical,
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
///
/// Encoding has two forms (see [`EncodeMode`]): the required [`bit_encode`](Self::bit_encode)
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

    /// The form [`EncodeExt::encode`] writes for this value. Defaults to
    /// [`EncodeMode::Verbatim`]; a `#[bin]` message with a `reserved`/`calc` field carries a
    /// settable `encode_mode` and overrides this to return it.
    fn encode_mode(&self) -> EncodeMode {
        EncodeMode::Verbatim
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
    ($($t:ty),* $(,)?) => {$(
        impl BitDecode for $t {
            #[inline]
            fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
                r.read::<$t>()
            }
        }
        impl BitEncode for $t {
            #[inline]
            fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError> {
                w.write(*self)
            }
        }
        // A leaf's fixed width is its `Bits::BITS`, so `#[bin]` can size it the same way it
        // sizes a fixed nested message — uniformly via `FixedBitLen`.
        impl FixedBitLen for $t {
            const BIT_LEN: u32 = <$t as Bits>::BITS;
        }
    )*};
}
bits_leaf_codec!(u8, u16, u32, u64, u128, bool);

impl<T, const N: usize> BitDecode for crate::int::UInt<T, N>
where
    crate::int::UInt<T, N>: Bits,
{
    #[inline]
    fn bit_decode<S: Source>(r: &mut S) -> Result<Self, BitError> {
        r.read::<Self>()
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
}

/// `encode(writer)` for any [`BitEncode`] message — encodes to a `Vec` (using the type's
/// [`LAYOUT`](BitEncode::LAYOUT)) in `self`'s [`encode_mode`](BitEncode::encode_mode) and
/// writes it to a [`std::io::Write`] sink. A blanket-implemented extension trait, so bring it
/// into scope (`use bnb::prelude::*` or `use bnb::EncodeExt`) to call `.encode(&mut w)`. Only
/// with the `std` feature; in `no_std` use the generated `to_bytes`/`to_canonical_bytes`, or
/// [`bit_encode`](BitEncode::bit_encode)/[`canonical_bit_encode`](BitEncode::canonical_bit_encode)
/// over a [`Sink`].
#[cfg(feature = "std")]
pub trait EncodeExt: BitEncode {
    /// Encodes `self` to any [`std::io::Write`] (socket, file, `Vec`) in the value's
    /// [`encode_mode`](BitEncode::encode_mode) — verbatim unless its mode is set to
    /// [`Canonical`](EncodeMode::Canonical). For an unconditional choice, use the inherent
    /// `to_bytes` (verbatim) / `to_canonical_bytes` (canonical) instead.
    ///
    /// # Errors
    /// [`ErrorKind::Io`] on a write failure, else the encode error.
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<(), BitError>
    where
        Self: Sized,
    {
        match self.encode_mode() {
            EncodeMode::Verbatim => {
                encode_to_writer_with(w, Self::LAYOUT, |bw| self.bit_encode(bw))
            }
            EncodeMode::Canonical => {
                encode_to_writer_with(w, Self::LAYOUT, |bw| self.canonical_bit_encode(bw))
            }
        }
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

// ---------------------------------------------------------------------------
// Entry-point helpers — the logic behind the `#[derive]`-generated inherent
// methods (`Type::decode`/`peek`/`decode_exact`/`encode`/`to_bytes`). Kept here
// so the logic lives in one place rather than monomorphized inline per type;
// doc-hidden because the public surface is the generated methods.
// ---------------------------------------------------------------------------

/// Decode every message from `bytes` into a `Vec`, with the message's own byte/bit order baked
/// in — bit-aware, so messages that don't end on byte boundaries reassemble correctly. Backs
/// `Type::decode_all`. The buffer must hold whole messages (a partial tail is an error).
///
/// # Errors
/// The first decode [`BitError`] (e.g. a truncated trailing message).
#[doc(hidden)]
pub fn decode_all<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<Vec<T>, BitError> {
    let mut r = BitReader::with_layout(bytes, layout);
    let mut out = Vec::new();
    while r.remaining_bits() > 0 {
        let before = r.bit_pos();
        out.push(T::bit_decode(&mut r)?);
        if r.bit_pos() == before {
            break; // a zero-width message would otherwise spin forever
        }
    }
    Ok(out)
}

/// A lazy iterator decoding successive `T` from `bytes` (layout baked in) until the buffer is
/// drained, ending after the first error if one occurs. Backs `Type::decode_iter`.
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
            Ok(v) => {
                stopped = r.bit_pos() == before; // stop if a zero-width message made no progress
                Some(Ok(v))
            }
            Err(e) => {
                stopped = true;
                Some(Err(e))
            }
        }
    })
}

/// Decodes one message from `bytes` without consuming the caller's buffer
/// (tail-tolerant). Backs `Type::peek`.
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

/// Decodes and requires every **whole byte** consumed; a sub-byte tail in the
/// final byte is treated as padding. Backs `Type::decode_exact`.
///
/// # Errors
/// [`ErrorKind::TrailingBytes`] if whole bytes remain, else the decode error.
#[doc(hidden)]
pub fn decode_exact<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    let mut r = BitReader::with_layout(bytes, layout);
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
pub fn encode_to_vec<T: BitEncode>(value: &T, layout: Layout) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_layout(layout);
    value.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// Encodes `value` to any [`std::io::Write`]. Backs [`EncodeExt::encode`].
///
/// # Errors
/// [`ErrorKind::Io`] on a write failure, else the encode error.
/// Encode to a [`std::io::Write`] over a caller-supplied encode closure — backs
/// [`EncodeExt::encode`] in either [`EncodeMode`] (the closure picks `bit_encode` vs
/// `canonical_bit_encode`).
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
    for b in &mut arr {
        *b = r.read_bits(8)? as u8;
    }
    Ok(arr)
}

/// Peeks up to `max` bytes without consuming them — reads them, then rewinds. Returns
/// however many are available (fewer than `max` at end-of-input). Backs variable-width
/// `#[bin]` enum magic dispatch (peek the longest magic, match a prefix, then seek past
/// the matched one). Like other seeking directives it bounds the generated `decode`
/// on [`SeekSource`]; a forward-only source fails at runtime with
/// [`ErrorKind::NotSeekable`].
///
/// # Errors
/// [`ErrorKind::NotSeekable`] if the source can't rewind.
#[doc(hidden)]
pub fn peek_bytes<S: Source>(r: &mut S, max: usize) -> Result<Vec<u8>, BitError> {
    let start = r.bit_pos();
    let mut out = Vec::with_capacity(max);
    for _ in 0..max {
        match r.read_bits(8) {
            Ok(b) => out.push(b as u8),
            Err(_) => break, // end of input — a shorter magic may still match
        }
    }
    r.seek_to_bit(start)?;
    Ok(out)
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

/// A *forward-only* bit reader over any [`std::io::Read`] — the streaming counterpart
/// to the in-memory [`BitReader`], for a stream you read once and don't seek.
///
/// It is bounded on `Read` **only, not `Seek`**, so it works over inputs that can't
/// seek (a socket, or a `&[u8]`, which is `Read` but not `Seek`). A message that needs
/// to seek (`#[br(restore_position)]`) won't decode through it — use a [`BufSource`] or
/// [`SeekReader`] for that. Reads up to 128 bits per call (the [`Source`] width
/// ceiling); running out mid-message yields [`ErrorKind::Incomplete`] ("read more and
/// retry").
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
    inner: R,
    /// Leftover bits from the last partially-consumed byte, right-aligned in the low
    /// `lead_bits` bits (MSB-first, so they are the *high* bits of the next read).
    /// Always fewer than 8.
    lead: u32,
    lead_bits: u32,
    /// Total bits consumed so far (for position-aware errors).
    pos: usize,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> StreamBitReader<R> {
    /// Wraps a byte source.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            lead: 0,
            lead_bits: 0,
            pos: 0,
        }
    }

    /// The total number of bits consumed so far.
    #[must_use]
    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    /// Reads `n` (`<= 128`) bits MSB-first, pulling bytes from the source as needed.
    ///
    /// # Errors
    /// [`ErrorKind::TooWide`] if `n > 128`; [`ErrorKind::Incomplete`] if the
    /// source runs out mid-field (read more and retry). Either carries the bit
    /// offset.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.pos,
            ));
        }
        let at = self.pos;
        // Build the result MSB-first, consuming the leftover bits then whole bytes.
        // The accumulator never holds more than `n` (<= 128) bits, so it can't
        // overflow — unlike a "shift bytes in, mask out" buffer, which is why the old
        // byte-accumulator capped at 64 and this caps at the full 128.
        let mut result: u128 = 0;
        let mut need = n;
        while need > 0 {
            if self.lead_bits == 0 {
                let mut b = [0u8; 1];
                if self.inner.read_exact(&mut b).is_err() {
                    // Ran out mid-field: "need more bytes" (buffer and retry), not a
                    // definitive end-of-input.
                    return Err(BitError::new(ErrorKind::Incomplete { needed: None }, at));
                }
                self.lead = u32::from(b[0]);
                self.lead_bits = 8;
            }
            let take = need.min(self.lead_bits);
            // The top `take` of the `lead_bits` leftover bits (MSB-first).
            let shift = self.lead_bits - take;
            let chunk = (self.lead >> shift) & ((1u32 << take) - 1);
            result = (result << take) | u128::from(chunk);
            self.lead_bits -= take;
            self.lead &= (1u32 << self.lead_bits) - 1; // keep the unconsumed low bits
            need -= take;
        }
        self.pos += n as usize;
        Ok(result)
    }

    /// Reads one [`Bits`] value (width `<= 128`) of its declared width.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        Ok(T::from_bits(self.read_bits(T::BITS)?))
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for StreamBitReader<R> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        StreamBitReader::read_bits(self, n)
    }
    fn bit_pos(&self) -> usize {
        self.pos
    }
}

/// A **seekable** [`Source`] over a forward `Read` (a socket): it *retains* the bytes
/// it has read, so a seek-using message (`restore_position`) works over a non-seekable
/// stream by seeking within the retained buffer, reading more on demand. It is
/// **bounded** — a retention `cap` (default 64 KiB) past which it errors
/// [`ErrorKind::BufferFull`] rather than buffering unboundedly. The
/// "continuously-receiving peer that also needs to seek" case.
///
/// # Examples
///
/// ```
/// use bnb::{bin, BufSource};
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Word { value: u32 }
///
/// let mut src = BufSource::new(&[0x12, 0x34, 0x56, 0x78][..]); // any `Read`
/// assert_eq!(Word::decode(&mut src).unwrap(), Word { value: 0x1234_5678 });
/// ```
#[cfg(feature = "std")]
#[derive(Clone, Debug)]
pub struct BufSource<R> {
    inner: R,
    buf: Vec<u8>,
    bit_pos: usize,
    cap: usize,
    layout: Layout,
    eof: bool,
}

#[cfg(feature = "std")]
impl<R: std::io::Read> BufSource<R> {
    /// Wraps `inner` with the default 64 KiB retention cap, MSB-first big-endian.
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self::with_cap(inner, 64 * 1024)
    }

    /// Wraps `inner` with a retention `cap` (bytes), MSB-first big-endian.
    #[must_use]
    pub fn with_cap(inner: R, cap: usize) -> Self {
        Self::with_cap_and_layout(inner, cap, Layout::default())
    }

    /// Wraps `inner` with a retention `cap` (bytes) and [`Layout`].
    #[must_use]
    pub fn with_cap_and_layout(inner: R, cap: usize, layout: Layout) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            bit_pos: 0,
            cap,
            layout,
            eof: false,
        }
    }

    /// Reads from `inner` until `buf` holds at least `byte_end` bytes (or EOF/cap).
    fn fill_to(&mut self, byte_end: usize) -> Result<(), BitError> {
        while self.buf.len() < byte_end && !self.eof {
            if self.buf.len() >= self.cap {
                return Err(BitError::new(
                    ErrorKind::BufferFull { cap: self.cap },
                    self.bit_pos,
                ));
            }
            let want = (byte_end - self.buf.len()).min(self.cap - self.buf.len());
            let start = self.buf.len();
            self.buf.resize(start + want, 0);
            match self.inner.read(&mut self.buf[start..]) {
                Ok(0) => {
                    self.buf.truncate(start);
                    self.eof = true;
                }
                Ok(got) => self.buf.truncate(start + got),
                Err(e) => {
                    self.buf.truncate(start);
                    return Err(BitError::new(ErrorKind::Io(e.kind()), self.bit_pos));
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> Source for BufSource<R> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.bit_pos,
            ));
        }
        let byte_end = (self.bit_pos + n as usize).div_ceil(8);
        self.fill_to(byte_end)?;
        if self.buf.len() < byte_end {
            return Err(BitError::new(
                ErrorKind::Incomplete {
                    needed: Some(byte_end - self.buf.len()),
                },
                self.bit_pos,
            ));
        }
        let acc = extract_bits(&self.buf, self.bit_pos, n as usize, self.layout.bit);
        self.bit_pos += n as usize;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        // Seek within the retained buffer; a later read fills more on demand.
        // Backward seeks (`restore_position`) hit already-retained bytes.
        self.bit_pos = pos;
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read> SeekSource for BufSource<R> {}

/// A **push/pull, bit-aware** decode buffer for incremental framing.
///
/// Feed bytes with [`push`](Self::push) as they arrive — from a socket, a channel, a callback,
/// anything that delivers bytes — and take whole messages off the front with [`pull`](Self::pull),
/// which returns `Ok(None)` when it needs more bytes (push more and call again).
///
/// Unlike a byte cursor (`bytes::BytesMut::advance`), `BitBuf` tracks a **bit** position, so a
/// stream of messages that *don't* end on byte boundaries (bit-packed frames) reassembles cleanly:
/// it reclaims the fully-consumed whole bytes and retains any partial trailing byte for the next
/// message. It's the *pushable*, in-memory counterpart to [`BufSource`] (which pulls from a
/// `Read`). `no_std`-compatible (`alloc` only).
///
/// `BitBuf` is also a [`SeekSource`], so it reads through the same [`decode`](crate::BitDecode)
/// entry points as every other cursor: `Type::decode(&mut bitbuf)` advances its cursor (then call
/// [`compact`](Self::compact) to reclaim). For streaming, prefer [`pull`](Self::pull) — it bakes
/// the message's own [`LAYOUT`](BitEncode::LAYOUT) (so `little`/`lsb` messages are always correct),
/// decodes **and** reclaims, and reports "need more bytes" as `Ok(None)`. The bare `Source` path
/// instead uses the buffer's own [`with_layout`](Self::with_layout) order (default msb/big).
///
/// ```
/// use bnb::{bin, BitBuf};
/// #[bin(big)]
/// #[derive(Debug, PartialEq, Eq)]
/// struct Ping { seq: u16 }
///
/// let mut bb = BitBuf::new();
/// bb.push(&[0x00]);                                  // only half of the first message
/// assert_eq!(bb.pull::<Ping>().unwrap(), None);      // not a whole message yet
/// bb.push(&[0x01, 0x00, 0x02]);                      // rest of msg 1 + all of msg 2
/// assert_eq!(bb.pull::<Ping>().unwrap(), Some(Ping { seq: 1 }));
/// assert_eq!(bb.pull::<Ping>().unwrap(), Some(Ping { seq: 2 }));
/// assert_eq!(bb.pull::<Ping>().unwrap(), None);      // drained
/// ```
///
#[derive(Debug, Default, Clone)]
pub struct BitBuf {
    /// Buffered bytes; everything before `cursor`'s byte is reclaimed by `pull`/`compact`.
    buf: Vec<u8>,
    /// Live read position, in bits, into `buf` (`0..=buf.len() * 8`).
    cursor: usize,
    /// Byte/bit order for the [`Source`] impl (the `decode(&mut bitbuf)` path); default msb/big.
    layout: Layout,
}

impl BitBuf {
    /// An empty buffer (msb/big order for the [`Source`] path).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty buffer with room for `cap` bytes before reallocating.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
            cursor: 0,
            layout: Layout::default(),
        }
    }

    /// Set the byte/bit order used by the [`Source`] impl (the `decode(&mut bitbuf)` path).
    /// [`pull`](Self::pull) ignores this — it always bakes the message's own `LAYOUT`.
    #[must_use]
    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Append freshly-received bytes to the back of the buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The number of unconsumed bits currently buffered.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.buf.len() * 8 - self.cursor
    }

    /// Whether no unconsumed bits remain.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bit_len() == 0
    }

    /// Drop all buffered bytes and reset the cursor.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    /// Reclaim the fully-consumed whole bytes (drop everything before the cursor's byte), keeping
    /// any partial trailing byte. [`pull`](Self::pull) does this for you; call it yourself when
    /// consuming via the [`Source`] path (`decode(&mut bitbuf)`) to keep the buffer bounded.
    pub fn compact(&mut self) {
        let whole = self.cursor / 8;
        self.buf.drain(..whole);
        self.cursor -= whole * 8;
    }

    /// Decode the next complete message off the front, reclaiming the bytes it consumed.
    ///
    /// Returns `Ok(None)` when the buffer doesn't yet hold a whole message — push more bytes and
    /// call again; the cursor is left untouched, so the retry is free. A malformed message is an
    /// `Err`. The byte/bit order is taken from `T`'s [`LAYOUT`](BitEncode::LAYOUT), so it decodes
    /// `little`/`lsb` messages correctly regardless of [`with_layout`](Self::with_layout).
    ///
    /// # Errors
    /// A codec [`BitError`] for a malformed message.
    pub fn pull<T: BitDecode + BitEncode>(&mut self) -> Result<Option<T>, BitError> {
        if self.cursor >= self.buf.len() * 8 {
            return Ok(None);
        }
        let mut r = BitReader::with_layout(&self.buf, <T as BitEncode>::LAYOUT);
        r.seek_to_bit(self.cursor)?;
        match T::bit_decode(&mut r) {
            Ok(msg) => {
                self.cursor = r.bit_pos();
                self.compact(); // reclaim consumed whole bytes, keep any partial trailing byte
                Ok(Some(msg))
            }
            // Only a partial message is buffered — wait for more (cursor untouched, retry-safe).
            Err(e)
                if matches!(
                    e.kind,
                    ErrorKind::UnexpectedEof { .. } | ErrorKind::Incomplete { .. }
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

impl Source for BitBuf {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let mut r = BitReader::with_layout(&self.buf, self.layout);
        r.seek_to_bit(self.cursor)?;
        let v = r.read_bits(n)?;
        self.cursor = r.bit_pos();
        Ok(v)
    }

    fn bit_pos(&self) -> usize {
        self.cursor
    }

    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }

    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        // Validate against the buffered bits (mirrors BitReader's bounds), then move the cursor.
        let mut probe = BitReader::with_layout(&self.buf, self.layout);
        probe.seek_to_bit(pos)?;
        self.cursor = pos;
        Ok(())
    }
}

impl SeekSource for BitBuf {}

/// A [`SeekSource`] over a seekable reader (`Read + Seek`, e.g. a `File`): it seeks
/// via [`std::io::Seek`] to the byte holding the bit cursor, **without buffering** —
/// the large-file / container-format case. For a *non*-seekable stream that still
/// needs to seek, use [`BufSource`].
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
        }
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> Source for SeekReader<R> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 128 {
            return Err(BitError::new(
                ErrorKind::TooWide { width: n as usize },
                self.bit_pos,
            ));
        }
        let bit_off = self.bit_pos % 8;
        let byte_start = (self.bit_pos / 8) as u64;
        let nbytes = (bit_off + n as usize).div_ceil(8);
        self.inner
            .seek(std::io::SeekFrom::Start(byte_start))
            .map_err(|e| BitError::new(ErrorKind::Io(e.kind()), self.bit_pos))?;
        let mut buf = vec![0u8; nbytes];
        self.inner.read_exact(&mut buf).map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                ErrorKind::UnexpectedEof {
                    needed: n as usize,
                    remaining: 0,
                }
            } else {
                ErrorKind::Io(e.kind())
            };
            BitError::new(kind, self.bit_pos)
        })?;
        let acc = extract_bits(&buf, bit_off, n as usize, self.layout.bit);
        self.bit_pos += n as usize;
        Ok(acc)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.layout.byte
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        self.bit_pos = pos; // the actual `io::Seek` happens on the next read
        Ok(())
    }
}

#[cfg(feature = "std")]
impl<R: std::io::Read + std::io::Seek> SeekSource for SeekReader<R> {}

/// Zero-copy `bytes`-crate adapters (the `bytes` feature): own a `Bytes` frame to
/// decode, encode into a `BytesMut` you `freeze()` to a `Bytes` — the async/tokio
/// framing case. Off by default so the core stays dependency-light.
#[cfg(feature = "bytes")]
mod bytes_io {
    use super::{BitError, BitReader, BitWriter, ByteOrder, Layout, SeekSource, Sink, Source};

    /// A [`SeekSource`](super::SeekSource) that **owns** a `bytes::Bytes` frame (no
    /// borrow), decoding bits from it. Constructing it from a `Bytes` is a refcount
    /// bump (zero copy).
    #[derive(Clone, Debug)]
    pub struct BytesReader {
        data: bytes::Bytes,
        bit_pos: usize,
        layout: Layout,
    }

    impl BytesReader {
        /// Owns `data`, positioned at bit 0, MSB-first big-endian.
        #[must_use]
        pub fn new(data: bytes::Bytes) -> Self {
            Self::with_layout(data, Layout::default())
        }

        /// Owns `data` with the given [`Layout`](super::Layout).
        #[must_use]
        pub fn with_layout(data: bytes::Bytes, layout: Layout) -> Self {
            Self {
                data,
                bit_pos: 0,
                layout,
            }
        }
    }

    impl Source for BytesReader {
        fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
            let mut br = BitReader::with_layout(&self.data, self.layout);
            br.seek_to_bit(self.bit_pos)?;
            let v = br.read_bits(n)?;
            self.bit_pos = Source::bit_pos(&br);
            Ok(v)
        }
        fn bit_pos(&self) -> usize {
            self.bit_pos
        }
        fn byte_order(&self) -> ByteOrder {
            self.layout.byte
        }
        fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
            self.bit_pos = pos;
            Ok(())
        }
    }

    impl SeekSource for BytesReader {}

    /// A [`Sink`](super::Sink) that encodes into a `bytes::BytesMut`; [`freeze`]
    /// hands off a zero-copy `Bytes`.
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

        /// An empty writer in the given [`Layout`](super::Layout).
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

    impl Sink for BytesWriter {
        fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
            self.inner.write_bits(value, n)
        }
        fn bit_pos(&self) -> usize {
            Sink::bit_pos(&self.inner)
        }
        fn byte_order(&self) -> ByteOrder {
            Sink::byte_order(&self.inner)
        }
    }
}

#[cfg(feature = "bytes")]
pub use bytes_io::{BytesReader, BytesWriter};

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

    // --- BitError: Display for every ErrorKind, and the offset/field suffix --------

    use alloc::string::{String, ToString};

    #[test]
    fn display_unexpected_eof() {
        let e = BitError::new(
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8,
            },
            0,
        );
        assert_eq!(
            e.to_string(),
            "unexpected end of input: needed 16 bits, 8 remain at bit 0"
        );
    }

    #[test]
    fn display_incomplete_with_and_without_hint() {
        assert_eq!(
            BitError::new(ErrorKind::Incomplete { needed: Some(3) }, 8).to_string(),
            "incomplete: need ~3 more bytes at bit 8",
        );
        assert_eq!(
            BitError::new(ErrorKind::Incomplete { needed: None }, 8).to_string(),
            "incomplete: need more bytes at bit 8",
        );
    }

    #[test]
    fn display_trailing_too_wide_not_seekable_buffer_full() {
        assert_eq!(
            BitError::new(ErrorKind::TrailingBytes { remaining: 2 }, 16).to_string(),
            "2 trailing bytes after the message at bit 16",
        );
        assert_eq!(
            BitError::new(ErrorKind::TooWide { width: 129 }, 0).to_string(),
            "field width 129 exceeds the 128-bit carrier at bit 0",
        );
        assert_eq!(
            BitError::new(ErrorKind::NotSeekable, 4).to_string(),
            "a position directive ran on a non-seekable source at bit 4",
        );
        assert_eq!(
            BitError::new(ErrorKind::BufferFull { cap: 64 }, 0).to_string(),
            "buffered source exceeded its 64-byte cap at bit 0",
        );
    }

    #[test]
    fn display_bad_magic_and_convert() {
        assert_eq!(
            BitError::bad_magic(0xCAFE, 0x0000, 0).to_string(),
            "bad magic: expected 0xcafe, found 0x0 at bit 0",
        );
        assert_eq!(
            BitError::convert(String::from("nope"), 8).to_string(),
            "conversion failed: nope at bit 8",
        );
    }

    #[test]
    fn display_appends_field_span_when_set() {
        let e = BitError::new(ErrorKind::TooWide { width: 200 }, 12).in_field("payload");
        assert_eq!(
            e.to_string(),
            "field width 200 exceeds the 128-bit carrier at bit 12 (field `payload`)"
        );
    }

    #[test]
    fn display_io_kind() {
        let e = BitError::new(ErrorKind::Io(std::io::ErrorKind::BrokenPipe), 0);
        assert!(e.to_string().starts_with("I/O error:"));
    }

    // --- BitError constructors and the two From bridges ----------------------------

    #[test]
    fn in_field_records_only_the_innermost() {
        let e = BitError::new(ErrorKind::NotSeekable, 0)
            .in_field("inner")
            .in_field("outer"); // ignored — inner already set
        assert_eq!(e.field, Some("inner"));
    }

    #[test]
    fn is_incomplete_is_true_only_for_incomplete() {
        assert!(BitError::new(ErrorKind::Incomplete { needed: None }, 0).is_incomplete());
        assert!(!BitError::new(ErrorKind::NotSeekable, 0).is_incomplete());
    }

    #[test]
    fn construction_error_bridges_to_a_convert_error() {
        let e: BitError = crate::error::Error::ValueTooLarge { value: 99, bits: 4 }.into();
        assert!(matches!(e.kind, ErrorKind::Convert { .. }));
        assert_eq!(e.at, 0);
        assert!(e.to_string().contains("does not fit in 4 bits"));
    }

    #[test]
    fn io_error_bridges_to_an_io_kind() {
        let e: BitError = std::io::Error::new(std::io::ErrorKind::TimedOut, "x").into();
        assert_eq!(e.kind, ErrorKind::Io(std::io::ErrorKind::TimedOut));
        assert_eq!(e.at, 0);
    }

    // --- BitWriter: the LSB-order constructor and the over-wide guard ---------------

    #[test]
    fn writer_with_order_lsb_packs_first_field_in_the_low_bits() {
        let mut w = BitWriter::with_order(BitOrder::Lsb);
        w.write(u4::new(0xA)).unwrap(); // -> low nibble
        w.write(u4::new(0xB)).unwrap(); // -> high nibble
        assert_eq!(w.into_bytes(), [0xBA]);
    }

    #[test]
    fn write_bits_rejects_over_128() {
        let mut w = BitWriter::new();
        assert_eq!(
            w.write_bits(0, 129).unwrap_err().kind,
            ErrorKind::TooWide { width: 129 }
        );
    }

    // --- Source/Sink trait DEFAULT methods, via minimal in-test impls --------------

    /// A forward-only `Source` that overrides only the two required methods, so calling
    /// the rest exercises the trait's default `byte_order`/`seek_to_bit`/`read`.
    struct TinySource<'a> {
        bytes: &'a [u8],
        pos: usize,
    }
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
    }

    #[test]
    fn source_default_byte_order_is_big() {
        let s = TinySource {
            bytes: &[0],
            pos: 0,
        };
        assert_eq!(s.byte_order(), ByteOrder::Big);
    }

    #[test]
    fn source_default_seek_is_not_seekable() {
        let mut s = TinySource {
            bytes: &[0, 0],
            pos: 0,
        };
        assert_eq!(s.seek_to_bit(8).unwrap_err().kind, ErrorKind::NotSeekable);
    }

    #[test]
    fn source_default_read_dispatches_through_read_bits() {
        let mut s = TinySource {
            bytes: &[0xAB, 0xCD],
            pos: 0,
        };
        assert_eq!(s.read::<u8>().unwrap(), 0xAB);
        assert_eq!(s.read::<u8>().unwrap(), 0xCD);
    }

    /// A `Sink` that overrides only the required methods, exercising the default
    /// `byte_order`/`write`.
    struct TinySink {
        out: Vec<u8>,
        bit: usize,
    }
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

    // --- BitEncode/DecodeWith defaults for a leaf type -----------------------------

    #[test]
    fn leaf_canonical_encode_defaults_to_verbatim() {
        let mut a = BitWriter::new();
        let mut b = BitWriter::new();
        BitEncode::bit_encode(&0xABCDu16, &mut a).unwrap();
        BitEncode::canonical_bit_encode(&0xABCDu16, &mut b).unwrap();
        assert_eq!(a.into_bytes(), b.into_bytes());
    }

    #[test]
    fn leaf_encode_mode_default_is_verbatim() {
        assert_eq!(BitEncode::encode_mode(&0u16), EncodeMode::Verbatim);
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
