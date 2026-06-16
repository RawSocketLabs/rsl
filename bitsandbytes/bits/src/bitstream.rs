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
//! The wire [`Layout`] is configurable: bit order (MSB-first default — bit 0 is the
//! high bit of byte 0, the RFC/ETSI convention — or LSB-first) and byte order (big-
//! endian default, or little-endian for byte-multiple values).
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

use crate::field::{BitOrder, Bits, ByteOrder};

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
    /// A `magic` constant read off the wire did not match. Both values are the
    /// type-erased low-bit representations ([`Bits::into_bits`](crate::Bits::into_bits)).
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
    /// values ([`Bits::into_bits`](crate::Bits::into_bits)).
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

impl std::error::Error for BitError {}

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
/// resolving to a bit count. Bring it in with `use bits::prelude::*`.
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Bit packing order — does the first bit land in the high or low bit.
    pub bit: BitOrder,
    /// Byte order, applied to byte-multiple values.
    pub byte: ByteOrder,
}

/// Reverses the low `bits / 8` bytes of `raw` when little-endian and the width is a
/// whole number of bytes (binrw applies byte order only to byte-multiple types); a
/// no-op for big-endian or sub-byte widths. It is its own inverse, so read and
/// write share it.
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

/// A cursor that reads values at arbitrary bit offsets from a byte slice, in a
/// chosen [`BitOrder`] (MSB-first by default — `bit 0` is the high bit of byte 0,
/// the RFC/ETSI ASCII-art convention; LSB-first for serial/PHY layers).
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

    /// Reads one [`Bits`] value of its declared width, applying the byte order to a
    /// byte-multiple value.
    ///
    /// # Errors
    /// As [`read_bits`](Self::read_bits).
    pub fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(apply_byte_order(raw, T::BITS, self.byte)))
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

    /// Appends one [`Bits`] value of its declared width, applying the byte order to
    /// a byte-multiple value.
    ///
    /// # Errors
    /// As [`write_bits`](Self::write_bits).
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
    fn read<T: Bits>(&mut self) -> Result<T, BitError> {
        let raw = self.read_bits(T::BITS)?;
        Ok(T::from_bits(apply_byte_order(
            raw,
            T::BITS,
            self.byte_order(),
        )))
    }
}

/// A [`Source`] that can seek (its [`seek_to_bit`](Source::seek_to_bit) is real, not
/// the failing default) — the bound for a message using a position directive
/// (`restore_position`). Implemented by the in-memory [`BitReader`]; a `Read + Seek`
/// adapter is Phase 3b.
pub trait SeekSource: Source {}

impl SeekSource for BitReader<'_> {}

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

    /// The byte order applied to a byte-multiple value (default big-endian).
    fn byte_order(&self) -> ByteOrder {
        ByteOrder::Big
    }

    /// Appends one [`Bits`] value of its declared width, applying the byte order.
    ///
    /// # Errors
    /// As [`write_bits`](Sink::write_bits).
    fn write<T: Bits>(&mut self, value: T) -> Result<(), BitError> {
        let raw = apply_byte_order(value.into_bits(), T::BITS, self.byte_order());
        self.write_bits(raw, T::BITS)
    }
}

impl Source for BitReader<'_> {
    fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        BitReader::read_bits(self, n)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
    fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        BitReader::seek_to_bit(self, pos)
    }
}

impl Sink for BitWriter {
    fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        BitWriter::write_bits(self, value, n)
    }
    fn bit_pos(&self) -> usize {
        self.bit_pos
    }
    fn byte_order(&self) -> ByteOrder {
        self.byte
    }
}

/// A message decoded from a bit stream — the recursion point a
/// `#[derive(BitDecode)]` struct implements (reading each field in declaration
/// order). Leaf fields are any [`Bits`] type; nested messages recurse. Fixed- or
/// variable-length; a fixed-length message *also* implements [`FixedBitLen`].
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
/// in a byte stream (the binrw bridge / `#[bitwire]`). A `count`-bearing message
/// implements [`BitDecode`]/[`BitEncode`] but **not** this.
pub trait FixedBitLen {
    /// Total encoded width of the message in bits — the sum of its fields' widths.
    const BIT_LEN: u32;
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
pub trait BitEncode {
    /// Encodes `self` into any [`Sink`], advancing its cursor.
    ///
    /// # Errors
    /// Propagates the sink's [`BitError`].
    fn bit_encode<K: Sink>(&self, w: &mut K) -> Result<(), BitError>;
}

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

/// Decodes one message from the front of `buf`, advancing `buf` past the bytes
/// consumed (the tail stays in `buf`). Transactional: on error `buf` is
/// unchanged. Backs `Type::decode`.
///
/// # Errors
/// Propagates the decode [`BitError`].
#[doc(hidden)]
pub fn decode_consume<T: BitDecode>(buf: &mut &[u8], layout: Layout) -> Result<T, BitError> {
    let input = core::mem::take(buf);
    let mut r = BitReader::with_layout(input, layout);
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
pub fn decode_peek<T: BitDecode>(bytes: &[u8], layout: Layout) -> Result<T, BitError> {
    T::bit_decode(&mut BitReader::with_layout(bytes, layout))
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

/// Encodes `value` to any [`std::io::Write`]. Backs `Type::encode`.
///
/// # Errors
/// [`ErrorKind::Io`] on a write failure, else the encode error.
#[doc(hidden)]
pub fn encode_to_writer<T: BitEncode, W: std::io::Write>(
    value: &T,
    w: &mut W,
    layout: Layout,
) -> Result<(), BitError> {
    let mut bw = BitWriter::with_layout(layout);
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

/// A **seekable** [`Source`] over a forward `Read` (a socket): it *retains* the
/// bytes it has read, so a seek-using message (`restore_position`) works over a
/// non-seekable stream by seeking within the retained buffer, reading more on
/// demand. It is **bounded** — a retention `cap` (default 64 KiB) past which it
/// errors [`ErrorKind::BufferFull`] rather than buffering unboundedly. The
/// "continuously-receiving peer that also needs to seek" case (DD3/DD4).
#[derive(Clone, Debug)]
pub struct BufSource<R> {
    inner: R,
    buf: Vec<u8>,
    bit_pos: usize,
    cap: usize,
    layout: Layout,
    eof: bool,
}

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
        let mut acc = 0u128;
        for k in 0..n as usize {
            let p = self.bit_pos + k;
            let byte = self.buf[p >> 3];
            match self.layout.bit {
                BitOrder::Msb => acc = (acc << 1) | u128::from((byte >> (7 - (p & 7))) & 1),
                BitOrder::Lsb => acc |= u128::from((byte >> (p & 7)) & 1) << k,
            }
        }
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

impl<R: std::io::Read> SeekSource for BufSource<R> {}

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
        T: BitDecode + super::FixedBitLen,
        R: Read + Seek,
    {
        let pos = reader.stream_position()?;
        let n = (<T as super::FixedBitLen>::BIT_LEN as usize).div_ceil(8);
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
