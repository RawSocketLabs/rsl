//! The I/O ladder — where the codec reads from and writes to.
//!
//! The everyday entry points (`decode_exact`/`decode_all`/`peek`/`to_bytes`) work on byte
//! slices and `Vec`s. When you need to read from a socket or a file, `decode` takes any
//! [`Source`](crate::Source) cursor; to write into an explicit [`Sink`](crate::Sink),
//! [`BitEncode::bit_encode`](crate::BitEncode::bit_encode) does the dual.
//! Pick the source by what your input can do:
//!
//! | Source | Backing | Can seek? | Use for |
//! |---|---|---|---|
//! | [`BitReader`](crate::BitReader) | `&[u8]` slice | yes (free cursor math) | in-memory bytes |
//! | `StreamBitReader` (`std`) | any `Read` | no (forward only) | a stream you read once |
//! | `BufSource` (`std`) | any `Read` | yes (within a bounded buffer) | a socket that also needs to seek |
//! | [`BitBuf`](crate::BitBuf) | owned `Vec<u8>` (pushable) | yes (cursor math) | incremental framing: push bytes, pull messages |
//! | `SeekReader` (`std`) | `Read + Seek` | yes (via `io::Seek`) | a large file / container |
//! | `BytesReader` (`bytes` feature) | owned `Bytes` | yes | zero-copy async framing |
//!
//! Seeking is only needed by messages that use `#[br(restore_position)]`; everything
//! else runs over the forward-only `StreamBitReader` too.
//!
//! <div class="warning">
//!
//! **`decode` reads in the *source's* layout, not the message's.** A [`Source`](crate::Source)
//! carries its own byte/bit order, and `Type::decode(&mut source)` reads in *that* order.
//! The plain constructors (`StreamBitReader::new`,
//! `BufSource::new`, `SeekReader::new`)
//! default to **msb/big** — correct for a default-layout message, but a non-default
//! (`little`/`lsb`) message decoded through them is **silently misread**. Build the source
//! with the message's layout: `StreamBitReader::with_layout(r, <Msg as bnb::BitEncode>::LAYOUT)`
//! (likewise `SeekReader::with_layout` and `BufSource::with_capacity_and_layout`). The slice
//! entry points (`decode_exact`/`decode_all`/`peek`) sidestep this — they bake `Msg`'s layout
//! in — so they are the foolproof "decode this buffer" path.
//!
//! </div>
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
//! an explicit bit sink (for composing into a cursor you already hold). `encode` is always
//! **verbatim**; for the canonical form encode `value.to_canonical()` — see
//! [Two encode forms](super::bin_codec#two-encode-forms-verbatim-vs-canonical).
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
//! (`BufSource`/`SeekReader`). The reverse —
//! handing a bnb cursor to `std::io`-based code from a `parse_with`/`write_with` — is
//! `Source::as_read` and `Sink::as_write`,
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
//! Use [`BitBuf`](crate::BitBuf) when bytes arrive incrementally. Each attempt either
//! consumes one complete message or leaves **all** input/cursor state unchanged. Drain
//! repeatedly for zero, one, or many messages; on `Incomplete`, return to the caller and
//! resume after more bytes arrive. No iterator or collection of outputs is required.
//!
//! When bytes arrive in pieces from something that *isn't* a `Read` (a channel, a callback, an
//! async chunk), [`BitBuf`](crate::BitBuf) is the **push/pull** counterpart: `push(&bytes)` as
//! they come, `pull::<T>()` to take whole messages off the front (it returns `None` until a full
//! message is buffered). `BitBuf` is itself a [`SeekSource`](crate::SeekSource), so it also reads
//! through plain [`decode`](crate::BitDecode) (`Type::decode(&mut bitbuf)`); `pull` adds the
//! reclaim + layout-baking + `None`-on-incomplete on top. Reclaim is deferred and in place, so a
//! push/pull loop reuses one allocation; for a guaranteed-fixed footprint use
//! [`BitBuf::bounded(cap)`](crate::BitBuf::bounded) with [`push`](crate::BitBuf::push)
//! (which refuses to grow) and [`grow`](crate::BitBuf::grow) for explicit resizing.
//!
//! ```
//! use bnb::{bin, BitBuf, BitError};
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Record {
//!     #[brw(count_prefix = u16)]
//!     body: Vec<u8>,
//! }
//! let mut input = BitBuf::bounded(64);
//! input.push(&[0, 3, 10]).unwrap();
//! let need = input.try_pull::<Record>().unwrap_err();
//! assert!(need.is_incomplete()); // also carries `at`, `field`, and an optional byte hint
//! input.push(&[20, 30, 0, 0]).unwrap(); // rest of record 1 and all of record 2
//! let mut messages = Vec::new();
//! loop {
//!     match input.try_pull::<Record>() {
//!         Ok(record) => messages.push(record),
//!         Err(error) if error.is_incomplete() => break,
//!         Err(error) => return Err(error),
//!     }
//! }
//! assert_eq!(messages, [Record { body: vec![10, 20, 30] }, Record { body: vec![] }]);
//! assert!(input.pull_eof::<Record>()?.is_none()); // finite, clean EOF
//! # Ok::<(), BitError>(())
//! ```
//!
//! `pull` remains the convenient `Result<Option<T>, BitError>` form. `try_pull` exposes
//! the shortfall details. `pull_eof` declares just this attempt finite: empty input is
//! `None`, truncation is a hard error, and a later push is still allowed. Logical bounded
//! regions and codec errors never masquerade as physical backing exhaustion.
//!
//! **Costs and limits.** A byte hint describes the next blocked operation, not the final
//! frame size or a no-read-ahead promise. Positive hints obey the strict lower-bound
//! contract in [`ErrorKind::Incomplete`](crate::ErrorKind::Incomplete); they are not estimates.
//! Attempts replay parsing, not suspended parser
//! state; callbacks/context must tolerate retries and their side effects are not undone.
//! A generated context-free `Vec<u8>` checks the complete body before allocating under
//! incremental decoding. General variable-element collections can still do quadratic work
//! under one-byte fragmentation. Batch input and bound frame size/attempt frequency for
//! such grammars. Buffer caps bound retained wire bytes, **not** allocations owned by
//! decoded values, recursion, or arbitrary custom-codec work.
//!
//! Positions are local to the buffer; compaction can rebase them. Extract an owned
//! length-delimited envelope before parsing formats with message-relative pointers
//! (e.g. compressed DNS), or before lending bytes to a borrowed parser (e.g. DER).
//! `try_pull_with(layout, args)` / `pull_eof_with(layout, args)` support `DecodeWith<A>`
//! and read-only codecs; arguments are resupplied each attempt with no `Clone` bound.
//!
//! `StreamBitReader` and direct `Source` decoding are **not transactional**. A failed
//! forward read may already have consumed input. `BufSource` retains bytes but its caller
//! must explicitly rewind for a retry. Neither is a substitute for `BitBuf` attempts.
//!
//! # Lossless protocol handoff
//!
//! | Surface | Message boundary | Safe handoff |
//! |---|---|---|
//! | `BitBuf` | Exact bits, including tightly packed messages | Keep the buffer and cursor |
//! | `MessageStream` (`net`) | Independently byte-padded messages | Read raw bytes through the wrapper, or transfer `into_parts` / `from_parts` |
//! | `BinCodec` + Tokio `Framed` (`tokio`) | Independently byte-padded messages | Transfer **both** `FramedParts::read_buf` and `write_buf` |
//!
//! `MessageStream::try_into_inner` succeeds only with no unread bits, otherwise it returns
//! the entire wrapper. Its raw `Read` drains buffered bytes immediately without also
//! touching the underlying reader; an imported unaligned bit cursor errors without
//! consumption (except an empty destination, which returns zero). `Write`/`flush` delegate.
//! Direct reads through `get_ref`/`get_mut` bypass buffering and can reorder input. A bounded
//! message read never reads beyond its remaining retention capacity, so a capacity failure
//! cannot discard newly read bytes. Write failures may have sent a prefix; do not retry a
//! whole message blindly. Sync helpers do not flush implicitly.
//!
//! Both transport conveniences still obtain `T::LAYOUT` from `BitEncode`; contextual or
//! read-only transport convenience is not added here. `BinCodec::decode_eof` makes a finite
//! attempt, retaining truncated input on error. Never drop a `Framed` read/write buffer
//! during a phase change: `Framed::into_inner` alone is lossy.
//!
//! # Reading *and* writing one connection (without `try_clone`)
//!
//! To run a request/response loop on a single TCP connection you need to read and write the
//! same socket. You don't need `try_clone()` (which dups the fd): **`std`'s `&TcpStream`
//! implements both `std::io::Read` and `std::io::Write`**, so wrap the read
//! half in a `BufSource` and write through `&TcpStream` — two shared borrows
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
//! not bytes) — and `MessageDatagram`, the datagram counterpart over a sealed `DatagramSocket`
//! (`UdpSocket` or `UnixDatagram`) with `send_message`/`recv_message`; both are unit-testable
//! without a real socket via the **`mock`** feature. With **`tokio`**, `BinCodec` does the same
//! for an async `Framed` stream.
//!
//! ## Borrowed whole-message readers (`net` / `tokio-io`)
//!
//! Already own the stream and retained buffer? The borrowed helpers keep that ownership:
//!
//! ```
//! # #[cfg(feature = "net")] {
//! use bnb::{bin, BitBuf};
//! use bnb::net::read_message;
//! #[bin(big)]
//! struct Envelope { #[brw(count_prefix = u16)] body: Vec<u8> }
//! let mut input = &b"\x00\x03abcRAW"[..];
//! let mut buffer = BitBuf::bounded(64);
//! let mut scratch = [0; 64]; // reuse across messages and protocol phases
//! let message: Envelope = read_message(&mut input, &mut buffer, &mut scratch)?;
//! assert_eq!(message.body, b"abc");
//! // Move BOTH transport and retained bytes into the next protocol owner.
//! let mut tunnel = bnb::MessageStream::from_parts(input, buffer);
//! let mut raw = [0; 3];
//! std::io::Read::read_exact(&mut tunnel, &mut raw)?;
//! assert_eq!(&raw, b"RAW");
//! # }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ```
//! # #[cfg(feature = "tokio-io")]
//! # async fn example() -> Result<(), bnb::net::MessageReadError> {
//! use bnb::{bin, BitBuf};
//! #[bin(big)]
//! struct Record { sequence: u32 }
//! let mut input = &b"\x00\x00\x00\x07"[..];
//! let mut buffer = BitBuf::bounded(64);
//! let mut scratch = [0; 64];
//! let record: Record = bnb::net::read_message_async(
//!     &mut input, &mut buffer, &mut scratch,
//! ).await?;
//! assert_eq!(record.sequence, 7);
//! # Ok(()) }
//! ```
//!
//! Both helpers try buffered decoding first, then wait for the additional-byte lower bound
//! before replaying the codec. They still allow read-ahead and immediately retain every
//! successful read. EOF triggers a finite decode even with an unmet hint. Dropping a pending
//! async call retains bytes from completed reads; retry with the same buffer and transport.
//! This is not resumable parser state or a cancellation-safe protocol session. Caller-owned
//! deadlines, output allocations, and general variable-element replay costs remain unchanged.
//!
//! **Migration from 0.5:** `MessageStream::read_message` now returns
//! `net::MessageReadError::{Codec(BitError), Io(io::Error)}`. Match the outer variant before
//! inspecting a codec's `kind` or an I/O error's `kind()`. Ordinary transport failures preserve
//! the original owned source; no downcast is needed to distinguish codec from I/O. Writes and
//! datagrams still return `BitError`. Audit custom positive hints against the strengthened
//! contract above; use `None` for speculative or input-length/state-dependent guesses.
//! `BinCodec` remains stateless and suitable for both streams and datagrams; it does not cache
//! hints. Core `BitBuf` and direct `Source` remain available for application-owned reading.
