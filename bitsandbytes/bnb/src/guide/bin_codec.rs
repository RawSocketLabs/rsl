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
//! | encode | `to_bytes() -> Vec<u8>` | encode to a fresh buffer |
//! | encode | `encode(&mut W)` | encode to any [`std::io::Write`] |
//! | encode | `encode_into(&mut K)` | encode into an explicit [`Sink`](crate::Sink) |
//! | build | `builder()` | the required-by-default builder |
//!
//! `decode`/`peek`/`decode_exact`/`to_bytes` are the everyday slice/`Vec` path;
//! `decode_from`/`encode_into` open the door to the [I/O ladder](super::io).
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
//! See [`directives`](super::directives) for every field directive, and
//! [`io`](super::io) for decoding from a socket or file rather than a slice. The
//! `examples/bin_message.rs` example in the repository is a runnable version of the
//! header + frame above.
