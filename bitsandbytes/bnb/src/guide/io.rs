//! The I/O ladder — where the codec reads from and writes to.
//!
//! The everyday entry points (`decode`/`peek`/`decode_exact`/`to_bytes`) work on byte
//! slices and `Vec`s. When you need to read from a socket or a file, `decode`
//! takes any [`Source`](crate::Source); to write into an explicit [`Sink`](crate::Sink),
//! [`BitEncode::bit_encode`](crate::BitEncode::bit_encode) does the dual.
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
//! # `decode` over each source
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
//! assert_eq!(Word::decode(&mut r).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a forward-only `Read` (a `&[u8]` is `Read` but not `Seek`)
//! let mut s = StreamBitReader::new(&bytes[..]);
//! assert_eq!(Word::decode(&mut s).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a `Read` with a bounded retain-and-seek buffer (the socket case)
//! let mut b = BufSource::new(&bytes[..]);
//! assert_eq!(Word::decode(&mut b).unwrap(), Word { value: 0x1234_5678 });
//!
//! // a `Read + Seek` (a file; here a Cursor over a Vec)
//! let mut f = SeekReader::new(Cursor::new(bytes.to_vec()));
//! assert_eq!(Word::decode(&mut f).unwrap(), Word { value: 0x1234_5678 });
//! ```
//!
//! # Encoding
//!
//! `to_bytes()` (the common case) returns a `Vec`; `encode(&mut impl Write)` writes straight
//! to a socket or file, and [`bit_encode(&mut impl Sink)`](crate::BitEncode::bit_encode) targets
//! an explicit bit sink (for composing into a cursor you already hold). `encode` follows the
//! value's [`encode_mode`](crate::EncodeMode) — **verbatim** by default, or **canonical** if set
//! — see [Two encode forms](super::bin_codec#two-encode-forms-verbatim-vs-canonical).
//!
//! ```
//! use bnb::bin;
//! use bnb::EncodeExt; // brings `.encode(&mut impl Write)` into scope (the `std` feature)
//! # #[bin(big)] #[derive(Debug, PartialEq)] struct Word { value: u32 }
//! let w = Word { value: 0x1234_5678 };
//! assert_eq!(w.to_bytes().unwrap(), [0x12, 0x34, 0x56, 0x78]);
//!
//! let mut out: Vec<u8> = Vec::new();   // any std::io::Write
//! w.encode(&mut out).unwrap();         // Word has no canonical form → always verbatim
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
//!
//! When bytes arrive in pieces from something that *isn't* a `Read` (a channel, a callback, an
//! async chunk), [`BitBuf`](crate::BitBuf) is the **push/pull** counterpart: `push(&bytes)` as
//! they come, `pull::<T>()` to take whole messages off the front (it returns `None` until a full
//! message is buffered). `BitBuf` is itself a [`SeekSource`](crate::SeekSource), so it also reads
//! through plain [`decode`](crate::BitDecode) (`Type::decode(&mut bitbuf)`); `pull` adds the
//! reclaim + layout-baking + `None`-on-incomplete on top.
//!
//! # Reading *and* writing one connection (without `try_clone`)
//!
//! To run a request/response loop on a single TCP connection you need to read and write the
//! same socket. You don't need `try_clone()` (which dups the fd): **`std`'s `&TcpStream`
//! implements both [`Read`](std::io::Read) and [`Write`](std::io::Write)**, so wrap the read
//! half in a [`BufSource`](crate::BufSource) and write through `&TcpStream` — two shared borrows
//! of the *same* socket:
//!
//! ```no_run
//! use bnb::{bin, BufSource};
//! use std::io::Write;
//! use std::net::TcpStream;
//! # #[bin(big)] #[derive(Debug, PartialEq)] struct Msg { seq: u32 }
//! let stream = TcpStream::connect("127.0.0.1:9000")?;
//! let mut reader = BufSource::new(&stream); // &TcpStream: Read
//! let mut writer = &stream;                 // &TcpStream: Write — the same socket
//!
//! writer.write_all(&Msg { seq: 1 }.to_bytes()?)?;
//! let reply = Msg::decode(&mut reader)?; // one framed message off the stream
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! For halves you need to **move across threads** (a dedicated reader thread and writer thread),
//! the equivalent of tokio's `into_split` is `Arc<TcpStream>` — clone the `Arc` per side and use
//! `&*arc` (still `Read + Write`), no `try_clone`. The runnable `examples/tcp.rs` shows a full
//! client/server.
//!
//! For an ergonomic wrapper, the **`net` feature** adds `MessageStream` — it owns a `Read +
//! Write` stream and exposes `read_message`/`write_message` (so you exchange `#[bin]` values,
//! not bytes) — and `MessageDatagram`, the datagram counterpart over any `DatagramSocket`
//! (`UdpSocket`, `UnixDatagram`, …) with `send_message`/`recv_message`. With **`tokio`**,
//! `BinCodec` does the same for an async `Framed` stream.
