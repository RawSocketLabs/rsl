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

use crate::field::Bits;

/// An error from the bit-level codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BitError {
    /// Asked for more bits than remain in the input.
    UnexpectedEof {
        /// Bits requested.
        needed: usize,
        /// Bits still available.
        remaining: usize,
    },
    /// A single field exceeded the 128-bit carrier width.
    TooWide(usize),
}

impl fmt::Display for BitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BitError::UnexpectedEof { needed, remaining } => write!(
                f,
                "unexpected end of input: needed {needed} bits, {remaining} remain"
            ),
            BitError::TooWide(n) => {
                write!(f, "field width {n} exceeds the 128-bit carrier")
            }
        }
    }
}

impl std::error::Error for BitError {}

/// A big-endian / MSB-first cursor that reads values at arbitrary bit offsets
/// from a byte slice.
#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    /// Wraps `bytes`, positioned at bit 0.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
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
    /// [`BitError::TooWide`] if `n > 128`; [`BitError::UnexpectedEof`] if fewer
    /// than `n` bits remain.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::TooWide(n));
        }
        if n > self.remaining_bits() {
            return Err(BitError::UnexpectedEof {
                needed: n,
                remaining: self.remaining_bits(),
            });
        }
        let mut acc: u128 = 0;
        for _ in 0..n {
            let byte = self.bytes[self.bit_pos >> 3];
            let bit = (byte >> (7 - (self.bit_pos & 7))) & 1;
            acc = (acc << 1) | u128::from(bit);
            self.bit_pos += 1;
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
    /// [`BitError::UnexpectedEof`] if `pos` is past the end of the buffer.
    pub fn seek_to_bit(&mut self, pos: usize) -> Result<(), BitError> {
        if pos > self.bytes.len() * 8 {
            return Err(BitError::UnexpectedEof {
                needed: pos,
                remaining: self.bytes.len() * 8,
            });
        }
        self.bit_pos = pos;
        Ok(())
    }

    /// Advances the cursor to the next byte boundary (a no-op if already aligned).
    pub fn align_to_byte(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }
}

/// A big-endian / MSB-first sink that appends values at arbitrary bit offsets,
/// growing a byte buffer (the final partial byte is zero-padded).
#[derive(Clone, Debug, Default)]
pub struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: usize,
}

impl BitWriter {
    /// An empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bits written so far.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.bit_pos
    }

    /// Appends the low `n` (`<= 128`) bits of `value`, MSB-first.
    ///
    /// # Errors
    /// [`BitError::TooWide`] if `n > 128`.
    pub fn write_bits(&mut self, value: u128, n: u32) -> Result<(), BitError> {
        let n = n as usize;
        if n > 128 {
            return Err(BitError::TooWide(n));
        }
        for i in (0..n).rev() {
            let byte_idx = self.bit_pos >> 3;
            if byte_idx == self.bytes.len() {
                self.bytes.push(0);
            }
            if (value >> i) & 1 != 0 {
                self.bytes[byte_idx] |= 1 << (7 - (self.bit_pos & 7));
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

/// A message decoded from a bit stream — the recursion point a
/// `#[derive(BitDecode)]` struct implements (reading each field in declaration
/// order). Leaf fields are any [`Bits`] type.
pub trait BitDecode: Sized {
    /// Total bit width of the message — the sum of its fields' widths. The derive
    /// computes it from `<Field as Bits>::BITS`; used to size a byte region when
    /// the message is embedded in a byte stream (see `#[bitwire]`).
    const BIT_LEN: u32;

    /// Decodes `Self` from `r`, advancing its cursor.
    ///
    /// # Errors
    /// Propagates the reader's [`BitError`].
    fn bit_decode(r: &mut BitReader<'_>) -> Result<Self, BitError>;
}

/// A message encoded to a bit stream — the dual of [`BitDecode`].
pub trait BitEncode {
    /// Encodes `self` into `w`, advancing its cursor.
    ///
    /// # Errors
    /// Propagates the writer's [`BitError`].
    fn bit_encode(&self, w: &mut BitWriter) -> Result<(), BitError>;
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
}

impl<R: std::io::Read> StreamBitReader<R> {
    /// Wraps a byte source.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            acc: 0,
            nbits: 0,
        }
    }

    /// Reads `n` (`<= 64`) bits MSB-first, pulling bytes from the source as needed.
    ///
    /// # Errors
    /// [`BitError::TooWide`] if `n > 64`; [`BitError::UnexpectedEof`] if the source
    /// runs out mid-field.
    pub fn read_bits(&mut self, n: u32) -> Result<u128, BitError> {
        if n > 64 {
            return Err(BitError::TooWide(n as usize));
        }
        while self.nbits < n {
            let mut b = [0u8; 1];
            self.inner
                .read_exact(&mut b)
                .map_err(|_| BitError::UnexpectedEof {
                    needed: n as usize,
                    remaining: self.nbits as usize,
                })?;
            self.acc = (self.acc << 8) | u128::from(b[0]);
            self.nbits += 8;
        }
        let shift = self.nbits - n;
        let take = if n == 0 { 0 } else { (1u128 << n) - 1 };
        let val = (self.acc >> shift) & take;
        self.nbits = shift;
        let keep = if shift == 0 { 0 } else { (1u128 << shift) - 1 };
        self.acc &= keep;
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
        assert_eq!(
            r.read_bits(8),
            Err(BitError::UnexpectedEof {
                needed: 8,
                remaining: 4
            })
        );
    }
}
