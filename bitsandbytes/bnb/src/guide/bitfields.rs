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
//! assert_eq!(b.raw(), 0x45);          // the classic IPv4 first byte
//! assert_eq!(b.version().value(), 4);
//! ```
//!
//! # Generated API
//!
//! For a field `f: T`, you get `f() -> T`, `with_f(T) -> Self` (consuming, chainable),
//! and `set_f(&mut self, T)`. Plus `new()` (all-zero), `raw()`/`from_raw()`, and
//! allocation-free `to_be_bytes`/`to_le_bytes`/`from_be_bytes`/`from_le_bytes`. The
//! type also implements [`Bits`](crate::Bits) and [`Bitfield`](crate::Bitfield), so
//! it nests in another bitfield or a `#[bin]` message.
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
//! - `bytes = be | le` (default `be`): the byte order of the backing integer when
//!   serialized to bytes.
//!
//! They are orthogonal. The same fields, packed `msb` but emitted `le`:
//!
//! ```
//! use bnb::{bitfield, u4};
//!
//! #[bitfield(u16, bits = msb, bytes = be)]
//! #[derive(Clone, Copy)]
//! struct Be { hi: u4, mid: u8, lo: u4 }
//!
//! #[bitfield(u16, bits = msb, bytes = le)]
//! #[derive(Clone, Copy)]
//! struct Le { hi: u4, mid: u8, lo: u4 }
//!
//! let be = Be::new().with_hi(u4::new(0xA)).with_mid(0xBC).with_lo(u4::new(0xD));
//! let le = Le::new().with_hi(u4::new(0xA)).with_mid(0xBC).with_lo(u4::new(0xD));
//! assert_eq!(be.to_be_bytes(), [0xAB, 0xCD]); // same logical value...
//! assert_eq!(le.to_le_bytes(), [0xCD, 0xAB]); // ...different wire bytes
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
