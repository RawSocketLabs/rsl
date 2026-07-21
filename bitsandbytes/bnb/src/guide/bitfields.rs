//! `#[bitfield]` — pack typed fields into one backing integer.
//!
//! A `#[bitfield]` collapses a struct of `Bits`-typed fields into a single unsigned
//! integer, generating accessors that shift and mask. It is the tool for a run of
//! sub-byte fields that together fill one word (a flags/opcode byte, a VLAN tag, an
//! IPv4 first byte).
//!
//! ```
//! use bnb::{bitfield, u4};
//!
//! #[bitfield(u8, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct VersionIhl {
//!     version: u4,   // high nibble
//!     ihl: u4,       // low nibble
//! }
//!
//! let b = VersionIhl::new().with_version(u4::new(4)).with_ihl(u4::new(5));
//! assert_eq!(b.to_raw(), 0x45);          // the classic IPv4 first byte
//! assert_eq!(b.version().value(), 4);
//! ```
//!
//! # Generated API
//!
//! For a field `f: T`, you get `f() -> T`, `with_f(T) -> Self` (consuming, chainable),
//! and `set_f(&mut self, T)`. Plus `new()` (all-zero), `to_raw()`/`from_raw()`, and
//! allocation-free byte conversions: `to_bytes`/`from_bytes` serialize in the **declared**
//! byte order (`bytes = big|little`), while `to_be_bytes`/`to_le_bytes`/`from_be_bytes`/`from_le_bytes`
//! force a specific endianness (the override). The type also implements [`Bits`](crate::Bits)
//! and [`Bitfield`](crate::Bitfield), so it nests in another bitfield or a `#[bin]` message.
//!
//! ```
//! use bnb::{bitfield, u4};
//! # #[bitfield(u8, bits = msb)] #[derive(Clone, Copy)] struct VersionIhl { version: u4, ihl: u4 }
//! let mut b = VersionIhl::new().with_version(u4::new(4)).with_ihl(u4::new(5));
//! b.set_ihl(u4::new(6));
//! assert_eq!(b.ihl().value(), 6);
//! assert_eq!(VersionIhl::from_be_bytes([0x45]).version().value(), 4);
//! ```
//!
//! # Bit order vs. byte order — two independent knobs
//!
//! - `bits = msb | lsb` (default `msb`): does the **first** declared field land in the
//!   high or low bits of the backing integer. `msb` matches the ASCII-art layouts in
//!   RFCs (first field drawn leftmost = most significant).
//! - `bytes = big | little` (default `big`): the byte order `to_bytes`/`from_bytes` use
//!   when serializing the backing integer.
//!
//! They are orthogonal *here*: a bitfield packs its fields into the backing integer (bit
//! order), then serializes that whole integer with the declared byte order — two genuinely
//! independent steps. (At the `#[bin]` *message* layer the two instead compose by the
//! natural-layout rule — a byte-multiple value is byte-swapped only when the declared byte
//! order differs from the bit order's natural layout; see the
//! [`bin_codec`](super::bin_codec) guide.) The same fields, packed `msb`, declared with two
//! different byte orders — `to_bytes` honors each declaration:
//!
//! ```
//! use bnb::{bitfield, u4};
//!
//! #[bitfield(u16, bits = msb, bytes = big)]
//! #[derive(Clone, Copy)]
//! struct Be { hi: u4, mid: u8, lo: u4 }
//!
//! #[bitfield(u16, bits = msb, bytes = little)]
//! #[derive(Clone, Copy)]
//! struct Le { hi: u4, mid: u8, lo: u4 }
//!
//! let be = Be::new().with_hi(u4::new(0xA)).with_mid(0xBC).with_lo(u4::new(0xD));
//! let le = Le::new().with_hi(u4::new(0xA)).with_mid(0xBC).with_lo(u4::new(0xD));
//! assert_eq!(be.to_bytes(), [0xAB, 0xCD]); // declared `big` -> big-endian bytes
//! assert_eq!(le.to_bytes(), [0xCD, 0xAB]); // same logical value, declared `little`
//!
//! // `to_be_bytes`/`to_le_bytes` ignore the declaration — use them only to override it.
//! assert_eq!(le.to_be_bytes(), [0xAB, 0xCD]);
//! ```
//!
//! # Field widths: inferred, explicit, or ranged
//!
//! In order of precedence:
//!
//! 1. **Inferred** (no attribute): the field's width is `<T as Bits>::BITS`. Fields
//!    pack adjacently in declaration order. This is the common case.
//! 2. **`#[bits(N)]`**: an explicit width, still auto-placed. Useful when a field's
//!    type is wider than the bits it should occupy.
//! 3. **`#[bits(A..=B)]`**: an absolute, inclusive bit range — fully manual layout,
//!    the equivalent of `bitbybit`'s `bits = A..=B`. Use ranges on **every** field, or
//!    on none; the two styles can't be mixed in one struct.
//!
//! With inferred widths the declared total is the sum of the fields (gaps are not
//! transmitted); with ranges the width is the whole backing integer (gaps are real
//! reserved bits on the wire).
//!
//! ```
//! use bnb::bitfield;
//!
//! // Manual layout: two fields with a deliberate 3-bit gap inside a u32.
//! #[bitfield(u32, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct Manual {
//!     #[bits(20..=31)] tag: bnb::u12,   // top 12 bits
//!     #[bits(0..=16)]  body: bnb::u17,  // low 17 bits; bits 17..=19 are reserved
//! }
//!
//! let m = Manual::new().with_tag(bnb::u12::new(0xABC)).with_body(bnb::u17::new(1));
//! assert_eq!(m.tag().value(), 0xABC);
//! assert_eq!(m.body().value(), 1);
//! ```
//!
//! A reversed range (`#[bits(31..=20)]`) is a clear compile error, not a silent
//! overflow, and field widths that exceed the backing integer fail a const assert.
//!
//! # Nesting
//!
//! Because a `#[bitfield]` is itself a `Bits` value, bitfields nest. A 5-bit field of
//! a nested type contributes exactly 5 bits to its parent:
//!
//! ```
//! use bnb::{bitfield, BitEnum, u3, u5};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u3)]
//! enum Op { Read, Write, #[catch_all] Other(u3) }
//!
//! #[bitfield(u8, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct Cmd { op: Op, addr: u5 }      // 3 + 5 = 8 bits exactly
//!
//! let c = Cmd::new().with_op(Op::Write).with_addr(u5::new(0x11));
//! assert_eq!(c.op(), Op::Write);
//! assert_eq!(c.addr().value(), 0x11);
//! ```
//!
//! See [`enums`](super::enums) and [`flags`](super::flags) for the field types that
//! nest here, and [`composition`](super::composition) for the full picture.
//!
//! # `#[view]` — a contextual typed view whose meaning depends on a sibling
//!
//! Some fields can't be interpreted from their own bits alone — the same bits mean
//! different things depending on a *sibling* field. (NXDN's LICH: two channel bits
//! read one way outbound and another inbound, with the direction bit alongside them.)
//! Because a `#[bitfield]` is random-access, a field's accessor can just read that
//! sibling — no cursor look-ahead. `#[view(bits = N, read = |raw, s| …, write = |v| …)]`
//! stores the raw `N` bits and materializes a typed value: `read` receives the raw bits
//! and `&Self` (call sibling getters for context), and `write` maps the typed value back
//! to raw bits (context-free). Both bridge through [`Bits`](crate::Bits), so the raw
//! type is inferred from the closures' own annotations — a `uN`, an enum, anything.
//!
//! ```
//! use bnb::{bitfield, u2, u3};
//!
//! #[derive(Debug, PartialEq, Eq, Clone, Copy)]
//! enum Kind { A, B, Other(u2) }
//! impl Kind {
//!     fn read(bits: u2, outbound: bool) -> Self {
//!         match (outbound, bits.value()) {
//!             (true, 0b00) => Kind::A,
//!             (false, 0b01) => Kind::B,
//!             _ => Kind::Other(bits),
//!         }
//!     }
//!     fn bits(self) -> u2 {
//!         match self { Kind::A => u2::new(0), Kind::B => u2::new(1), Kind::Other(b) => b }
//!     }
//! }
//!
//! #[bitfield(u8, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct Lich {
//!     header: u3,
//!     #[view(
//!         bits = 2,
//!         read = |raw: u2, s: &Self| Kind::read(raw, s.outbound()),
//!         write = |v: Kind| v.bits(),
//!     )]
//!     kind: Kind,
//!     outbound: bool,   // the context `kind` reads — a sibling
//!     trailing: u2,
//! }
//!
//! let outbound = Lich::new().with_kind(Kind::A).with_outbound(true);
//! assert_eq!(outbound.kind(), Kind::A);                     // outbound && bits 00 → A
//! // Same stored bits (00), different sibling → different meaning:
//! let inbound = Lich::new().with_kind(Kind::Other(u2::new(0))).with_outbound(false);
//! assert_eq!(inbound.kind(), Kind::Other(u2::new(0)));
//! ```
