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

#[cfg(doc)]
use crate::field::Bits;

mod amount;
mod bitbuf;
#[cfg(feature = "bytes")]
mod bytes_io;
mod codec;
#[cfg(test)]
mod component;
mod count_prefix;
mod cursor;
mod entry;
mod error;
#[cfg(test)]
mod fixtures;
mod incremental;
mod layout;
mod limited;
#[cfg(feature = "std")]
mod seek;
mod sink;
mod source;
#[cfg(feature = "std")]
mod stream;
mod support;

pub use amount::{BitAmount, align_read, align_write, skip_read, skip_write};
pub use bitbuf::{BitBuf, CapacityError};
#[cfg(feature = "tokio")]
pub(crate) use bytes_io::FrameWriter;
#[cfg(feature = "bytes")]
pub use bytes_io::{BytesReader, BytesWriter};
#[cfg(feature = "std")]
pub use codec::EncodeExt;
pub use codec::{
    BitDecode, BitEncode, DecodeWith, EncodeWith, FixedBitLen, WINDOW_LEAF, WINDOW_WHOLE,
    decode_vec_each, fixed_window, nested_vec, window_kind,
};
pub use count_prefix::CountPrefix;
pub use cursor::{BitReader, BitWriter};
#[cfg(feature = "net")]
pub(crate) use entry::encode_into;
#[cfg(feature = "std")]
pub use entry::encode_to_writer_with;
pub use entry::{
    decode_all, decode_exact, decode_exact_with, decode_iter, decode_peek, decode_peek_with,
    decode_prefix, decode_prefix_with, encode_fixed_to_vec, encode_hinted_to_vec, encode_to_vec,
    encode_to_vec_with,
};
pub use error::{
    BitError, Conversion, DispatchKind, DispatchValue, EnumDispatchError, ErrorKind,
    MAX_DECODE_DEPTH, MagicMismatch, MagicValue,
};
#[cfg(any(feature = "tokio", test))]
pub(crate) use incremental::decode_attempt;
pub use layout::Layout;
pub use limited::LimitedSource;
#[cfg(feature = "std")]
pub use seek::{BufSeekReader, SeekReader};
pub use sink::Sink;
#[cfg(feature = "std")]
pub use sink::SinkWriter;
#[cfg(feature = "std")]
pub use source::SourceReader;
pub use source::{SeekSource, Source};
#[cfg(feature = "std")]
pub use stream::{BufSource, StreamBitReader};
pub(crate) use support::invalid_utf8;
pub use support::{
    assert_message, bits_of, decode_mapped_msg, decode_try_mapped_msg, encode_mapped_msg,
    match_magic, peek_magic, prefix_for_count, read_byte_array, read_mapped, read_try_mapped,
    verify_magic, write_byte_array, write_mapped,
};

/// Seals [`Source`], [`Sink`], [`BitAmount`], [`CountPrefix`] and
/// [`codecs::leb128::Varint`](crate::codecs::leb128::Varint): each impl set is
/// crate-owned, so growing it later is non-breaking while a downstream impl can never
/// observe a bound change.
pub(crate) mod sealed {
    /// The sealing supertrait — unnameable downstream, so the traits above cannot be
    /// implemented outside this crate.
    pub trait Sealed {}
}
