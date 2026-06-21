//! The I/O ladder — where the codec reads from and writes to.
//!
//! The everyday entry points (`decode`/`peek`/`decode_exact`/`to_bytes`) work on byte
//! slices and `Vec`s. When you need to read from a socket or a file, `decode_from`
//! takes any [`Source`](crate::Source) and `encode_into` any [`Sink`](crate::Sink).
//! Pick the source by what your input can do:
//!
//! | Source | Backing | Can seek? | Use for |
//! |---|---|---|---|
//! | [`BitReader`](crate::BitReader) | `&[u8]` slice | yes (free cursor math) | in-memory bytes |
//! | [`StreamBitReader`](crate::StreamBitReader) | any `Read` | no (forward only) | a stream you read once |
//! | [`BufSource`](crate::BufSource) | any `Read` | yes (within a bounded buffer) | a socket that also needs to seek |
//! | [`SeekReader`](crate::SeekReader) | `Read + Seek` | yes (via `io::Seek`) | a large file / container |
//! | `BytesReader` (`bytes` feature) | owned `Bytes` | yes | zero-copy async framing |
//!
//! Seeking is only needed by messages that use `#[br(restore_position)]`; everything
//! else runs over the forward-only [`StreamBitReader`](crate::StreamBitReader) too.
//!
//! # The low-level cursor
//!
//! [`BitReader`](crate::BitReader)/[`BitWriter`](crate::BitWriter) are the bit cursors
//! under everything — read or write any [`Bits`](crate::Bits) value at the current bit
//! offset:
//!
//! ```
//! use bnb::{BitReader, BitWriter, u4, u12};
//!
//! let mut w = BitWriter::new();
//! w.write(u4::new(0xA)).unwrap();
//! w.write(u12::new(0xBCD)).unwrap();
//! assert_eq!(w.into_bytes(), [0xAB, 0xCD]);
//!
//! let mut r = BitReader::new(&[0xAB, 0xCD]);
//! assert_eq!(r.read::<u4>().unwrap(), u4::new(0xA));
//! assert_eq!(r.read::<u12>().unwrap(), u12::new(0xBCD));
//! ```
//!
//! # `decode_from` over each source
//!
//! The same message decodes from a slice cursor, a forward stream, a buffered socket,
//! or a seekable file — only the source type changes:
//!
//! ```
//! use bnb::{bin, BitReader, StreamBitReader, BufSource, SeekReader};
//! use std::io::Cursor;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Word { value: u32 }
//!
//! let bytes = [0x12, 0x34, 0x56, 0x78];
//!
//! // in-memory slice cursor
//! let mut r = BitReader::new(&bytes);
//! assert_eq!(Word::decode_from(&mut r).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a forward-only `Read` (a `&[u8]` is `Read` but not `Seek`)
//! let mut s = StreamBitReader::new(&bytes[..]);
//! assert_eq!(Word::decode_from(&mut s).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a `Read` with a bounded retain-and-seek buffer (the socket case)
//! let mut b = BufSource::new(&bytes[..]);
//! assert_eq!(Word::decode_from(&mut b).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a `Read + Seek` (a file; here a Cursor over a Vec)
//! let mut f = SeekReader::new(Cursor::new(bytes.to_vec()));
//! assert_eq!(Word::decode_from(&mut f).unwrap(), Word { value: 0x1234_5678 });
//! ```
//!
//! # Encoding
//!
//! `to_bytes()` is the common case; `encode(&mut impl Write)` goes straight to a
//! socket or file, and `encode_into(&mut impl Sink)` targets an explicit bit sink.
//!
//! ```
//! use bnb::bin;
//! use bnb::{EncodeExt, EncodeMode}; // `.encode(&mut impl Write, mode)` (the `std` feature)
//! # #[bin(big)] #[derive(Debug, PartialEq)] struct Word { value: u32 }
//! let w = Word { value: 0x1234_5678 };
//! assert_eq!(w.to_bytes().unwrap(), [0x12, 0x34, 0x56, 0x78]);
//!
//! let mut out: Vec<u8> = Vec::new();   // any std::io::Write
//! w.encode(&mut out, EncodeMode::Verbatim).unwrap();
//! assert_eq!(out, [0x12, 0x34, 0x56, 0x78]);
//! ```
//!
//! # The `bytes` feature
//!
//! With `--features bytes`, `BytesReader`/`BytesWriter` decode from / encode to the
//! `bytes` crate's `Bytes`/`BytesMut` for zero-copy async framing:
//!
//! ```ignore
//! // requires `bnb = { features = ["bytes"] }`
//! use bnb::{BytesReader, BytesWriter, Sink};
//! let mut w = BytesWriter::new();
//! w.write(0x1234u16).unwrap();
//! let frame = w.freeze();              // a zero-copy `bytes::Bytes`
//! let mut r = BytesReader::new(frame); // owns the frame, no copy
//! ```
//!
//! # Bridging to `std::io`
//!
//! The ladder above adapts a `std::io::Read` *into* a [`Source`](crate::Source)
//! ([`BufSource`](crate::BufSource)/[`SeekReader`](crate::SeekReader)). The reverse —
//! handing a bnb cursor to `std::io`-based code from a `parse_with`/`write_with` — is
//! [`Source::as_read`](crate::Source::as_read) and [`Sink::as_write`](crate::Sink::as_write),
//! byte views over the cursor. With `From<io::Error>`, `std::io` results `?` straight
//! into a [`BitError`](crate::BitError):
//!
//! ```
//! use bnb::{BitError, BitReader, Source};
//! use std::io::Read;
//!
//! fn read_three<S: Source>(r: &mut S) -> Result<[u8; 3], BitError> {
//!     let mut buf = [0u8; 3];
//!     r.as_read().read_exact(&mut buf)?; // a `std::io::Read` view over the cursor
//!     Ok(buf)
//! }
//!
//! let mut r = BitReader::new(&[0xAA, 0xBB, 0xCC]);
//! assert_eq!(read_three(&mut r).unwrap(), [0xAA, 0xBB, 0xCC]);
//! ```
//!
//! # Streaming and partial input
//!
//! A [`StreamBitReader`](crate::StreamBitReader) or [`BufSource`](crate::BufSource)
//! that runs out mid-message reports [`ErrorKind::Incomplete`](crate::ErrorKind), the
//! "read more bytes and retry" signal — distinct from a definitive parse failure. See
//! [`errors`](super::errors).
