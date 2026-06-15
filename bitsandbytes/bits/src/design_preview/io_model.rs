//! # I/O model — sources, sinks, seeking, streaming
//!
//! Two deliberate divergences from binrw: **no uniform `Seek` requirement** (and
//! no `NoSeek` wrapper), and a **polymorphic easy-button** so you never construct
//! a reader for the common case. (Target design; ` ```rust,ignore `.)
//!
//! ## One easy button over a `Source` / `Sink`
//!
//! `decode` takes anything that is a byte **`Source`**; `encode` writes to anything
//! that is a byte **`Sink`**:
//!
//! - `Source` is implemented for `&[u8]` (consumes from the front — transactional)
//!   and for any [`std::io::Read`] (sockets, files). Seeking is a separate
//!   per-source capability (`SeekSource`) — see *Sources & seeking* below.
//! - `Sink` is implemented for any [`std::io::Write`] (files, sockets, `Vec<u8>`)
//!   and for the in-memory [`BitWriter`](crate::BitWriter).
//!
//! ```rust,ignore
//! // Consume one message from a byte buffer; the slice advances past it.
//! let mut buf: &[u8] = &bytes;
//! let frame = Frame::decode(&mut buf)?;   // buf now holds any trailing bytes
//!
//! // Read straight from a socket/file (forward-only, no Seek):
//! let frame = Frame::decode(&mut tcp_stream)?;
//!
//! // Write to anything Write:
//! frame.encode(&mut tcp_stream)?;
//! frame.encode(&mut file)?;
//! let bytes = frame.to_bytes()?;          // convenience -> Vec<u8>
//! ```
//!
//! > **The consume idiom:** `&mut [u8]` (fixed length) can't shrink, so the
//! > consuming form is `&mut &[u8]` — a mutable view that re-points to the tail,
//! > exactly how `std::io::Read for &[u8]` advances. `decode(&mut buf)` "consumes
//! > the bytes it needs ahead of time," leaving the rest in `buf`.
//!
//! ## Entry points
//!
//! | Read | Source | Consumes | For |
//! |---|---|---|---|
//! | `decode(&mut impl Source)` | `&[u8]` / `Read` | yes (front) | the easy button; tail-tolerant; `Incomplete` ⇒ need more |
//! | `decode_exact(&[u8])` | full buffer | all-or-error | strict "this buffer is exactly one message" |
//! | `peek(&[u8])` | `&[u8]` | **no** | inspect the front without advancing |
//! | `decode_from(&mut BitReader)` | explicit cursor | yes | seeking, overlapping re-reads, many messages from one buffer |
//!
//! | Write | Sink | For |
//! |---|---|---|
//! | `encode(&mut impl Sink)` | any `Write` / `BitWriter` | the default |
//! | `to_bytes() -> Vec<u8>` | — | convenience |
//! | `encode_into(&mut BitWriter)` | explicit bit sink | embedding / bit control |
//!
//! ## "I just need more bytes" — `Incomplete`
//!
//! Over a `&[u8]`, `decode` is **transactional**: if the buffer is too short it
//! returns `Err(Incomplete { needed: Option<usize> })` and **leaves the slice
//! unchanged**, so you
//! append more bytes and retry — the framing loop:
//!
//! ```rust,ignore
//! // storage: a Vec<u8> you append socket reads to.
//! loop {
//!     let mut view: &[u8] = &storage;
//!     match Frame::decode(&mut view) {
//!         Ok(frame) => {
//!             let consumed = storage.len() - view.len();
//!             handle(frame);
//!             storage.drain(..consumed);          // drop what we used
//!         }
//!         Err(e) if e.is_incomplete() => break,   // read more, then loop
//!         Err(e) => return Err(e),                // a real parse error
//!     }
//! }
//! ```
//!
//! (A non-seekable `Read` can't be rewound, so the transactional retry above is the
//! slice form; reading directly from a socket means buffering into your own `Vec`
//! and decoding from a view of it — which is what the loop does.)
//!
//! ## Seeking & overlap — the explicit cursor
//!
//! Under the easy button sits `BitReader<'a>` over a `&'a [u8]`, tracking a **bit**
//! position. Because the whole message is in hand, seeking is just cursor
//! arithmetic — no `Seek` trait, no `NoSeek`:
//!
//! ```rust,ignore
//! let mut r = BitReader::new(&buf);
//! let a = Frame::decode_from(&mut r)?;   // advances the shared cursor
//! let b = Frame::decode_from(&mut r)?;   // next message, same buffer
//!
//! r.seek_to_bit(108);     // jump to an absolute bit offset (e.g. a DNS pointer)
//! r.align_to_byte();      // snap to the next byte boundary
//! let here = r.bit_pos(); // re-read overlapping bits, peek, patch — all explicit
//! ```
//!
//! This is the payoff of DD2 (`DESIGN.md` §11): `restore_position`, `seek`,
//! `pad`/`align`, and pointer-following all fall out for free with no `Read + Seek`
//! bound. It is also where **back-to-back bit-packed messages** (no byte boundary
//! between them) are decoded — the `Read`-based easy button is byte-granular, so a
//! message that ends mid-byte must continue through the same `BitReader`.
//!
//! **Tradeoff (accepted):** the seek/overlap path needs the message resident in a
//! buffer. For bounded protocol PDUs that is the normal case and enables zero-copy.
//!
//! ## Sources & seeking — the capability ladder
//!
//! Seeking is a property of the **source**, so a message's `decode` requires
//! exactly the capability it actually uses (the attribute-driven bound, DD3). Four
//! source tiers, one `decode`:
//!
//! | Source | Seek | bound it satisfies | Tier |
//! |---|---|---|---|
//! | `&[u8]` / [`BitReader`](crate::BitReader) (in-memory) | free (cursor math) | `Source` + `SeekSource` | easy |
//! | any [`std::io::Read`] (socket/pipe) | — | `Source` | easy |
//! | `BufSource<R: Read>` (socket adapter) | within its buffer | `SeekSource` | harder |
//! | `R: Read + Seek` (`File`) | via `io::Seek` + bit offset | `SeekSource` | long-run (`ROADMAP.md`) |
//!
//! **A forward-only message** ⇒ `decode(&mut impl Source)` — works on a slice, a
//! socket, or a file, no wrapper, no `Seek`. The normal case stays trivial.
//!
//! **A seek-using message** ⇒ `decode(&mut impl SeekSource)`. A slice and a `File`
//! satisfy it directly; a bare socket does **not** (you can't rewind a live
//! stream), so it is a *compile error* until you wrap it:
//!
//! ```rust,ignore
//! // Forward-only: socket or file directly, no wrapper.
//! let m = Telemetry::decode(&mut socket)?;
//!
//! // Seek-using message on a socket: wrap once. BufSource retains the bytes it
//! // reads, so backward seeks land in the buffer, and it pulls more on demand —
//! // handling both "need more" and "seek back" for a continuously-receiving peer.
//! let mut src = BufSource::new(socket).cap(64 * 1024);
//! let m = DnsMessage::decode(&mut src)?;     // compression pointers seek within
//!
//! // Seek-using message on a File: no buffering needed (it is Read + Seek).
//! let m = Archive::decode(&mut file)?;       // roadmap
//! ```
//!
//! `#[bin(forward_only)]` pins the `Source`-only bound and makes any seek directive
//! a compile error — the requirement is visible in the type, not discovered at the
//! call site.
//!
//! ### Safety: bounded buffering (do as much as possible, without the footgun)
//!
//! `BufSource` is the only thing that retains stream history, and it is
//! **bounded**: `cap(n)` (default = the framed message size) caps retained bytes,
//! and exceeding it is an `Err`, never unbounded growth — so "read from a socket
//! and seek" can't be turned into memory exhaustion by a hostile peer. `decode`
//! itself never silently buffers a non-seekable source; seek-over-socket is the
//! explicit, capped `BufSource` opt-in.
//!
//! ## Errors
//!
//! `bnb::Error` carries the **bit position** plus context:
//! `Incomplete { needed: Option<usize> }` (the streaming signal,
//! `e.is_incomplete()`), `BadMagic { at }`, `TrailingBytes { at }` (from
//! `decode_exact`), and validator messages. The runtime analogue of binrw's spans.
//!
//! > **✓ decided:** readers/writers are unified behind `Source`/`Sink`, with
//! > `BitReader`/`BitWriter` as the concrete in-memory implementations — one
//! > `decode`/`encode` covers slice, file, and socket (no distinct
//! > `decode_reader`).
//!
//! > **✓ decided:** `Incomplete { needed: Option<usize> }` — `needed` is a
//! > best-effort hint: `Some(n)` when a length prefix makes the shortfall knowable,
//! > `None` otherwise. Callers just read more and retry.
//!
//! > **✓ decided (seek API):** two layers — inherent `seek_to_bit`/`align_to_byte`
//! > on `BitReader` for the common in-memory case, **plus** a `SeekSource` trait
//! > (imported) implemented by the non-slice seekable sources (`BufSource`, `File`).
//! > A message is bounded `Source` (forward) or `SeekSource` (seeks). Seek-over-
//! > socket is the bounded `BufSource` adapter; large seekable files are designed
//! > here but roadmap'd, not MVP.
