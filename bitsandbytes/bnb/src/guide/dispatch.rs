//! `#[bin]` on an enum — tag-dispatched tagged unions.
//!
//! A protocol union carries a discriminant (a type/opcode/content-type) that selects
//! which payload follows. `#[bin]` on an enum reads that **tag** and dispatches to the
//! matching variant; each variant is a mini-struct whose fields use the same
//! `#[br]`/`#[bw]` grammar as a struct. Decode is a single forward `match` — no
//! backtracking.
//!
//! # Internal tag — read the discriminant, then dispatch
//!
//! `#[bin(tag = <ty>)]` reads a discriminant of that `Bits` type; each variant declares
//! its value with `#[bin(tag = <value>)]`. Variants may be unit, tuple, named, or
//! `#[nested]` (another `#[bin]` message).
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, tag = u16)]
//! #[derive(Debug, PartialEq)]
//! enum Rdata {
//!     #[bin(tag = 1)] A(u32),                  // tuple newtype
//!     #[bin(tag = 2)] Port { lo: u8, hi: u8 }, // struct variant
//!     #[bin(tag = 0)] Ping,                    // unit: tag only
//! }
//!
//! assert_eq!(Rdata::A(0x0808_0808).to_bytes().unwrap(), [0x00, 0x01, 8, 8, 8, 8]);
//! assert_eq!(
//!     Rdata::decode_exact(&[0x00, 0x02, 0x1A, 0x2B]).unwrap(),
//!     Rdata::Port { lo: 0x1A, hi: 0x2B },
//! );
//! assert_eq!(Rdata::Ping.to_bytes().unwrap(), [0x00, 0x00]);
//! ```
//!
//! # `#[catch_all]` — preserve an unknown tag (dual-use)
//!
//! The dual-use convention from [`#[derive(BitEnum)]`](super::enums) lifts to
//! data-carrying enums: a `#[catch_all]` variant captures an unrecognized tag (its
//! **first field**) plus the raw payload, so decode never rejects an unknown union and
//! encode round-trips it. Without a catch-all the union is *closed* — an unknown tag is
//! a decode error.
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, tag = u8)]
//! #[derive(Debug, PartialEq)]
//! enum Tlv {
//!     #[bin(tag = 1)] Hello,
//!     #[catch_all]
//!     Unknown { tag: u8, #[br(count = 2)] body: Vec<u8> },
//! }
//!
//! let v = Tlv::decode_exact(&[0x07, 0xAA, 0xBB]).unwrap();
//! assert_eq!(v, Tlv::Unknown { tag: 7, body: vec![0xAA, 0xBB] });
//! assert_eq!(v.to_bytes().unwrap(), [0x07, 0xAA, 0xBB]); // the unknown tag is preserved
//! ```
//!
//! # External tag — dispatch on context
//!
//! Often the discriminant is a separate field the parent already read. Declare it as
//! `ctx` and dispatch with `tag_from = <param>`; the enum then reads **no** tag of its
//! own, and the parent passes the value down with `#[br(ctx { … })]`.
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, ctx(kind: u16), tag_from = kind)]
//! #[derive(Debug, PartialEq)]
//! enum Body {
//!     #[bin(tag = 1)] Login(u32),
//!     #[bin(tag = 2)] Data { n: u8 },
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Packet {
//!     kind: u16,
//!     #[br(ctx { kind })]
//!     body: Body,
//! }
//!
//! let bytes = [0x00, 0x02, 0x2A];
//! assert_eq!(
//!     Packet::decode_exact(&bytes).unwrap(),
//!     Packet { kind: 2, body: Body::Data { n: 42 } },
//! );
//! ```
//!
//! # `tag()` — keep an external discriminant from drifting
//!
//! Every dispatched enum gets a `tag()` accessor returning the chosen variant's
//! discriminant, so a parent that stores the tag separately can recompute it instead of
//! risking drift: `#[bw(calc = self.body.tag())]`.
//!
//! ```
//! # use bnb::bin;
//! # #[bin(big, ctx(kind: u16), tag_from = kind)]
//! # #[derive(Debug, PartialEq)]
//! # enum Body { #[bin(tag = 1)] Login(u32), #[bin(tag = 2)] Data { n: u8 } }
//! assert_eq!(Body::Data { n: 0 }.tag(), 2);
//! assert_eq!(Body::Login(0).tag(), 1);
//! ```
//!
//! # Notes
//!
//! - Tag values are used as `match` patterns, so they must be literals or `const`
//!   paths (e.g. a `#[derive(BitEnum)]` variant for named tags).
//! - A `#[catch_all]` variant must store the tag in its first field so encode can put
//!   it back; the remaining fields are its payload (length usually from a `count`).
//! - Variant fields support the struct directives except `ctx`/`temp`/`calc` (not yet
//!   on variants).
