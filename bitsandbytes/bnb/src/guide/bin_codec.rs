//! `#[bin]` — the unified whole-message codec.
//!
//! `#[bin]` folds three things over one struct: the read codec, the write codec, and a
//! required-by-default builder. Fields are read and written at arbitrary **bit**
//! offsets, so the same attribute handles byte-aligned headers and sub-byte frames,
//! and any [`Bits`](crate::Bits) type — including a `#[bitfield]`, `#[derive(BitEnum)]`,
//! or `#[bitflags]` — drops in as one field with no glue.
//!
//! # A DNS header, end to end
//!
//! The 12-byte DNS message header (RFC 1035 §4.1.1) is a good tour: a 16-bit flags
//! word (itself a bitfield of enums and bools) between five `u16` counts/ids.
//!
//! ```
//! use bnb::{bin, bitfield, BitEnum, u3, u4};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u4)]
//! enum OpCode { Query, IQuery, Status, #[catch_all] Other(u4) }
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u4)]
//! enum RCode { NoError, FormErr, ServFail, NxDomain, #[catch_all] Other(u4) }
//!
//! // The 16-bit flags word, MSB-first (RFC diagram order), big-endian.
//! #[bitfield(u16, bits = msb, bytes = be)]
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Flags {
//!     qr: bool, opcode: OpCode, aa: bool, tc: bool,
//!     rd: bool, ra: bool, z: u3, rcode: RCode,   // 1+4+1+1+1+1+3+4 = 16
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Header {
//!     id: u16,
//!     flags: Flags,        // a 16-bit bitfield, nested as one field
//!     qdcount: u16,
//!     ancount: u16,
//!     nscount: u16,
//!     arcount: u16,
//! }
//!
//! // Build with the required-by-default builder.
//! let flags = Flags::new().with_qr(true).with_rd(true).with_ra(true);
//! let h = Header::builder()
//!     .id(0x1234).flags(flags)
//!     .qdcount(1).ancount(1).nscount(0).arcount(0)
//!     .build().unwrap();
//!
//! // Encode -> 12 bytes; decode is the exact inverse.
//! let bytes = h.to_bytes().unwrap();
//! assert_eq!(bytes.len(), 12);
//! assert_eq!(&bytes[..4], &[0x12, 0x34, 0x81, 0x80]); // id, then flags 0x8180
//! assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
//! ```
//!
//! # The generated API
//!
//! Every `#[bin]` type gets a consistent surface:
//!
//! | Direction | Method | Use |
//! |---|---|---|
//! | decode | `decode_exact(&[u8])` | one message that consumes every whole byte |
//! | decode | `decode(&mut &[u8])` | one message from the front; advances the slice |
//! | decode | `peek(&[u8])` | one message, tail-tolerant, no buffer mutation |
//! | decode | `decode_from(&mut S)` | from an explicit [`Source`](crate::Source) (stream/socket/file) |
//! | encode | `to_bytes() -> Vec<u8>` | encode to a fresh buffer (**verbatim**) |
//! | encode | `to_canonical_bytes()` | encode the spec-normalized form (**canonical**) † |
//! | encode | `encode(&mut W, mode)` | encode to any [`std::io::Write`] in an [`EncodeMode`](crate::EncodeMode) |
//! | encode | `encode_into(&mut K)` | encode (verbatim) into an explicit [`Sink`](crate::Sink) |
//! | build | `builder()` | the required-by-default builder |
//!
//! `decode`/`peek`/`decode_exact`/`to_bytes` are the everyday slice/`Vec` path;
//! `decode_from`/`encode(&mut W, mode)`/`encode_into` open the door to the
//! [I/O ladder](super::io). († `to_canonical_bytes` and the canonical helpers exist only
//! when the message has a `reserved` or `calc` field — see [Two encode forms](#two-encode-forms-verbatim-vs-canonical).)
//!
//! # Struct-level options
//!
//! Inside `#[bin(...)]`:
//!
//! - `big` / `little` — byte order (default `big`).
//! - `bit_order = msb | lsb` — bit order (default `msb`).
//! - `magic = <expr>` — a leading constant verified on read, emitted on write.
//! - `read_only` / `write_only` — generate only one direction.
//! - `no_builder` — skip the builder.
//! - `forward_only` — bound decoding to a forward `Source` (a seek directive is then a
//!   compile error).
//! - `ctx(name: Ty, …)` — declare context the message needs from its parent.
//! - `validate = <path>` — a soundness check run by `build()`.
//!
//! ## Each option, by example
//!
//! Byte and bit order:
//!
//! ```
//! use bnb::{bin, u4};
//!
//! #[bin(little)] // little-endian byte order
//! #[derive(Debug, PartialEq)]
//! struct Le { v: u32 }
//! assert_eq!(Le { v: 0x1234_5678 }.to_bytes().unwrap(), [0x78, 0x56, 0x34, 0x12]);
//!
//! #[bin(big, bit_order = lsb)] // the first field lands in the LOW bits of the byte
//! #[derive(Debug, PartialEq)]
//! struct Lsb { a: u4, b: u4 }
//! assert_eq!(Lsb { a: u4::new(0xA), b: u4::new(0xB) }.to_bytes().unwrap(), [0xBA]);
//! ```
//!
//! Directional codecs and the builder:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, read_only)] // only decodes — no `to_bytes`/`encode`
//! #[derive(Debug, PartialEq)]
//! struct Ro { v: u16 }
//! assert_eq!(Ro::decode_exact(&[0x12, 0x34]).unwrap(), Ro { v: 0x1234 });
//!
//! #[bin(big, write_only)] // only encodes — no `decode`/`peek`
//! struct Wo { v: u16 }
//! assert_eq!(Wo { v: 0x1234 }.to_bytes().unwrap(), [0x12, 0x34]);
//!
//! #[bin(big, no_builder)] // no `Nb::builder()` — construct directly
//! #[derive(Debug, PartialEq)]
//! struct Nb { v: u16 }
//! assert_eq!(Nb { v: 5 }.to_bytes().unwrap(), [0x00, 0x05]);
//! ```
//!
//! `forward_only` — decode from a non-seekable stream, with a compile-time no-seek
//! guarantee (a `#[br(restore_position)]` field would then be a compile error):
//!
//! ```
//! use bnb::{bin, StreamBitReader};
//!
//! #[bin(big, forward_only)]
//! #[derive(Debug, PartialEq)]
//! struct Hdr { magic: u16, len: u16 }
//!
//! let data: &[u8] = &[0xCA, 0xFE, 0x00, 0x10]; // `&[u8]` is `Read` but not `Seek`
//! let mut s = StreamBitReader::new(data);
//! assert_eq!(Hdr::decode_from(&mut s).unwrap(), Hdr { magic: 0xCAFE, len: 16 });
//! ```
//!
//! `ctx(...)` — context a message needs from its parent **to decode**. The parent passes it
//! with `#[br(ctx { … })]`; standalone, `decode_with`/`decode_with_exact` take a `…Ctx`
//! (built positionally with `…Ctx::new`). `ctx` is **decode-only**: encode stays a plain
//! `to_bytes()` unless the *write* side actually reads a ctx param (a keyed `bw(map)`,
//! `calc`, or `write_with`), in which case the type gets `to_bytes_with`/`encode_with`:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, ctx(len: u16))]
//! #[derive(Debug, PartialEq)]
//! struct Body {
//!     #[br(count = len)]
//!     data: Vec<u8>,
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Packet {
//!     len: u16,
//!     #[br(ctx { len })] // pass `len` to `Body`
//!     body: Body,
//! }
//!
//! let p = Packet { len: 3, body: Body { data: vec![1, 2, 3] } };
//! assert_eq!(p.to_bytes().unwrap(), [0x00, 0x03, 1, 2, 3]);
//! assert_eq!(Packet::decode_exact(&[0x00, 0x03, 1, 2, 3]).unwrap(), p);
//!
//! // Standalone: decode needs the context (build it with `BodyCtx::new`); encode is
//! // plain — `ctx` is decode-only, and `Body`'s write side doesn't read `len`.
//! let b = Body::decode_with_exact(&[0xAA, 0xBB], BodyCtx::new(2)).unwrap();
//! assert_eq!(b.data, vec![0xAA, 0xBB]);
//! assert_eq!(b.to_bytes().unwrap(), vec![0xAA, 0xBB]);
//! ```
//!
//! (`magic` and `validate` are shown below.)
//!
//! # Dual-use: `validate` gates the builder, not the parser
//!
//! `validate = path` runs a `fn(&Self) -> Result<(), impl Display>` in `build()` only.
//! The **parser stays permissive** — it never rejects representable input — so a
//! deliberately malformed message is still decodable (for fuzzing / interop), even
//! though it can't be *built*:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, validate = check)]
//! #[derive(Debug, PartialEq)]
//! struct Msg { kind: u8, len: u8 }
//!
//! fn check(m: &Msg) -> Result<(), String> {
//!     if m.kind == 0 { return Err("kind 0 is reserved".into()); }
//!     Ok(())
//! }
//!
//! assert!(Msg::builder().kind(0).len(4).build().is_err());      // builder: rejected
//! assert!(Msg::decode_exact(&[0x00, 0x04]).is_ok());            // parser: permissive
//! ```
//!
//! # Derived, never-drifting fields
//!
//! A length or count you don't want to store can be read into a temp local and
//! recomputed on write, so it can never disagree with the data. Here `len` drives a
//! `count`-bound `Vec` on read and is recomputed from `payload.len()` on write:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, magic = 0xCAFEu16)]
//! #[derive(Debug, PartialEq)]
//! struct Frame {
//!     #[br(temp)]
//!     #[bw(calc = self.payload.len() as u8)]
//!     len: u8,
//!     #[br(count = len)]
//!     payload: Vec<u8>,
//! }
//!
//! let f = Frame::builder().payload(vec![0xDE, 0xAD, 0xBE, 0xEF]).build().unwrap();
//! assert_eq!(f.to_bytes().unwrap(), [0xCA, 0xFE, 0x04, 0xDE, 0xAD, 0xBE, 0xEF]);
//! assert_eq!(Frame::decode_exact(&[0xCA, 0xFE, 0x02, 0x01, 0x02]).unwrap().payload, vec![1, 2]);
//! ```
//!
//! # Two encode forms: verbatim vs canonical
//!
//! A dual-use codec needs to do two opposite things: reproduce a message **exactly** (even a
//! malformed one you parsed off the wire), and emit a **spec-clean** one. So `#[bin]` gives
//! you both, and *never silently* picks for you:
//!
//! - **`to_bytes()` is verbatim** — it writes exactly what's stored. Retained `reserved` bits
//!   stay, a stored `calc` value is written as-is. This is the faithful inverse of `decode`:
//!   `decode` then `to_bytes` is byte-for-byte identical.
//! - **`to_canonical_bytes()` is canonical** — `reserved` fields are written as their spec
//!   value and `calc` fields are recomputed, so the result is always spec-compliant.
//!
//! The two differ only when a message has a `reserved` or non-`temp` `calc` field — so
//! `to_canonical_bytes` (and the three helpers below) are generated **only then**; otherwise
//! verbatim *is* canonical and only `to_bytes` exists. (`temp` + `calc` fields are never
//! stored, so they always recompute — they don't create a verbatim/canonical gap.)
//!
//! Three helpers inspect or normalize the value **in memory**, without encoding:
//! `is_canonical()`, `canonical_diff()` (the names of the fields that differ from canonical),
//! and `to_canonical(self) -> Self`. And when the choice is a *runtime* value (a config flag,
//! a CLI option), [`encode(&mut w, mode)`](crate::EncodeExt::encode) writes either form to any
//! `std::io::Write`.
//!
//! ```
//! use bnb::{bin, EncodeExt, EncodeMode};
//!
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Packet {
//!     tag: u8,
//!     #[reserved]
//!     rsv: u8,                       // spec value 0
//!     #[bw(calc = self.tag ^ 0x5A)]
//!     #[builder(default)]
//!     check: u8,                     // canonical value: tag ^ 0x5A
//! }
//!
//! // A value a peer sent us with non-spec reserved bits and a stale checksum:
//! let p = Packet { tag: 0x10, rsv: 0xFF, check: 0x99 };
//!
//! // VERBATIM — exactly what's stored (so decode -> to_bytes round-trips):
//! assert_eq!(p.to_bytes().unwrap(), [0x10, 0xFF, 0x99]);
//!
//! // CANONICAL — reserved -> 0, checksum recomputed (0x10 ^ 0x5A = 0x4A):
//! assert_eq!(p.to_canonical_bytes().unwrap(), [0x10, 0x00, 0x4A]);
//!
//! // Inspect the gap without encoding:
//! assert!(!p.is_canonical());
//! assert_eq!(p.canonical_diff(), ["rsv", "check"]);
//!
//! // Choose the form at runtime when writing to a socket/file:
//! let mut out: Vec<u8> = Vec::new();
//! p.encode(&mut out, EncodeMode::Canonical).unwrap();
//! assert_eq!(out, [0x10, 0x00, 0x4A]);
//! ```
//!
//! See [`directives`](super::directives) for every field directive, and
//! [`io`](super::io) for decoding from a socket or file rather than a slice. The
//! `examples/bin_message.rs` example in the repository is a runnable version of the
//! header + frame above.
