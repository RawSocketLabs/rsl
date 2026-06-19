//! `#[bin]` on an enum — tagged-union dispatch.
//!
//! A protocol union picks one of several payloads. `#[bin]` on an enum expresses that
//! with two **orthogonal** concepts:
//!
//! - **`magic`** — a wire constant that is *read and written*: a byte string (`b"IHDR"`)
//!   or a width-suffixed unsigned integer (`0x01u16`). It is the discriminant under
//!   *magic dispatch*, or a verified signature on a tag-variant.
//! - **`tag`** — a read-only **selector** taken from `ctx`. It picks the variant but is
//!   **never** on the wire.
//!
//! `#[catch_all]` preserves an unknown discriminant (dual-use); without one, a magic enum
//! is a *closed set* and an unknown discriminant is a decode error.
//!
//! # Magic dispatch — the discriminant is on the wire
//!
//! With no `tag`, each variant's `magic` is its discriminant: decode reads it once and
//! matches; encode writes it. Variants may be unit, tuple, named, or `#[nested]`.
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! enum Rdata {
//!     #[bin(magic = 1u16)] A(u32),
//!     #[bin(magic = 2u16)] Port { lo: u8, hi: u8 },
//!     #[bin(magic = 0u16)] Ping,
//!     #[catch_all]
//!     Other { magic: u16, #[br(count = 2)] raw: Vec<u8> }, // captures an unknown magic
//! }
//!
//! assert_eq!(Rdata::A(0x0808_0808).to_bytes().unwrap(), [0x00, 0x01, 8, 8, 8, 8]);
//! assert_eq!(Rdata::decode_exact(&[0x00, 0x09, 0xAA, 0xBB]).unwrap(),
//!            Rdata::Other { magic: 9, raw: vec![0xAA, 0xBB] });
//! assert_eq!(Rdata::A(0).magic(), 1); // the `magic()` accessor
//! ```
//!
//! Byte-string magics work the same way — the natural fit for PNG/RIFF-style signatures:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! enum Chunk {
//!     #[bin(magic = b"IHDR")] Header { width: u16, height: u16 },
//!     #[bin(magic = b"IDAT")] Data(u8),
//!     #[catch_all] Other { magic: [u8; 4], #[br(count = 1)] rest: Vec<u8> },
//! }
//!
//! let hdr = [b'I', b'H', b'D', b'R', 0x00, 0x10, 0x00, 0x20];
//! assert_eq!(Chunk::decode_exact(&hdr).unwrap(), Chunk::Header { width: 16, height: 32 });
//! assert_eq!(Chunk::Header { width: 16, height: 32 }.to_bytes().unwrap(), hdr);
//! ```
//!
//! A leading enum-level **`magic` prefix** is verified on read and written on encode,
//! once, before dispatch:
//!
//! ```
//! # use bnb::bin;
//! #[bin(big, magic = b"BNB")]
//! #[derive(Debug, PartialEq)]
//! enum Pre { #[bin(magic = 1u8)] A(u16), #[bin(magic = 2u8)] B }
//!
//! assert_eq!(Pre::A(0xCAFE).to_bytes().unwrap(), [b'B', b'N', b'B', 0x01, 0xCA, 0xFE]);
//! assert!(Pre::decode_exact(&[b'X', b'N', b'B', 0x01, 0, 0]).is_err()); // bad prefix
//! ```
//!
//! # Tag dispatch — a read-only selector, nothing on the wire
//!
//! Declare a selector with `tag = <ctx-param>` and give each variant `#[bin(tag = V)]`.
//! The enum reads/writes **no** discriminant; the parent passes the selector down with
//! `#[br(ctx { … })]`, and `tag()` recovers it (driving a no-drift `calc`).
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, ctx(kind: u16), tag = kind)]
//! #[derive(Debug, PartialEq)]
//! enum Body {
//!     #[bin(tag = 1)] Login(u32),
//!     #[bin(tag = 2)] Data { n: u8 },
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Packet {
//!     #[br(temp)]
//!     #[bw(calc = self.body.tag())] // recompute the tag from the chosen variant
//!     kind: u16,
//!     #[br(ctx { kind })]
//!     body: Body,
//! }
//!
//! let p = Packet { body: Body::Data { n: 7 } };
//! assert_eq!(p.to_bytes().unwrap(), [0x00, 0x02, 0x07]); // tag 2 then the payload
//! assert_eq!(Packet::decode_exact(&[0x00, 0x02, 0x07]).unwrap(), p);
//! ```
//!
//! # Composing `tag` + `magic`
//!
//! The two stack: the `tag` selects the variant, then its `magic` is a **signature** —
//! verified on read, written on encode (it *is* on the wire; the tag is not).
//!
//! ```
//! # use bnb::bin;
//! #[bin(big, ctx(kind: u8), tag = kind)]
//! #[derive(Debug, PartialEq)]
//! enum Msg {
//!     #[bin(tag = 1, magic = b"LI")] Login(u32), // verify "LI" after the tag picks it
//!     #[bin(tag = 2)] Ping,                       // no signature
//! }
//!
//! let li = [b'L', b'I', 0xAA, 0xBB, 0xCC, 0xDD];
//! assert_eq!(Msg::decode_with_exact(&li, MsgCtx { kind: 1 }).unwrap(), Msg::Login(0xAABB_CCDD));
//! assert!(Msg::decode_with_exact(&[b'X', b'X', 0, 0, 0, 0], MsgCtx { kind: 1 }).is_err());
//! ```
//!
//! # Decode helpers
//!
//! Beyond the usual entry points, a dispatched enum gets:
//!
//! - **`decode_as_<variant>(bytes)`** — parse the bytes *as one explicit variant* (its
//!   magic, if any, then its payload), bypassing dispatch. Handy when the variant is
//!   known out of band, and for tests. (A `ctx` enum takes the context too.)
//! - **`peek_variant(bytes) -> <Name>Kind`** (magic dispatch) — identify *which* variant
//!   the bytes are, from the wire magic, **without** parsing the payload — for routing.
//! - **`decode_tagged(selector, bytes)`** (tag dispatch) — feed the selector directly.
//!
//! ```
//! # use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! enum Op {
//!     #[bin(magic = 1u8)] Get(u16),
//!     #[bin(magic = 2u8)] Set { key: u8, val: u8 },
//! }
//!
//! assert_eq!(Op::peek_variant(&[0x02, 9, 9]).unwrap(), OpKind::Set);
//! assert_eq!(Op::decode_as_get(&[0x01, 0xAB, 0xCD]).unwrap(), Op::Get(0xABCD));
//! ```
//!
//! # Notes
//!
//! - **`magic` values are literals**: a byte string, or a width-suffixed unsigned integer
//!   (`1u16`, `0xCAu8`). Sub-byte and non-literal magics are rejected so the wire width is
//!   always unambiguous.
//! - A `#[catch_all]` stores the discriminant in its first field (the captured magic, or
//!   the selector under tag dispatch) so it can round-trip; its remaining fields are the
//!   payload (length usually from a `count`).
//! - Variant fields support the full directive grammar (`count`, `if`, `map`, `ctx`,
//!   `temp`/`calc`, `parse_with`, …), so a catch-all can read its own length and recompute
//!   it on encode.
//! - `tag()` (tag dispatch) / `magic()` (magic dispatch) return the variant's discriminant.
