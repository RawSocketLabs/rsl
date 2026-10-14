//! Composition — how the pieces nest and size each other.
//!
//! Everything in `bnb` is built on one idea: a value that occupies a fixed number of
//! bits ([`Bits`](crate::Bits)). Because that is the unit of composition, the macros
//! stack without any glue — a sub-byte enum nests in a packed word, which nests in a
//! byte-aligned message — and the widths are checked by the compiler, not guessed by
//! the macro.
//!
//! # The layers
//!
//! ```text
//!   #[bin] message            ← fields read/written at arbitrary bit offsets
//!     └─ #[bitfield] word      ← several Bits values packed into one integer
//!          ├─ #[derive(BitEnum)]   ← an integer discriminant (a Bits value)
//!          └─ #[bitflags] set      ← single-bit flags (a Bits value)
//!     └─ u1..u127 / bool / u8..    ← the leaf Bits values
//! ```
//!
//! Each arrow is "is a `Bits` value", so each layer is just a field of the one above.
//!
//! # A worked stack
//!
//! A 5-bit enum and a 3-bit flag set packed into one byte, with that byte a field of a
//! larger message — three layers, checked to fit at compile time:
//!
//! ```
//! use bnb::{bin, bitfield, bitflags, BitEnum, u5};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u5)]
//! enum Kind { Hello, Data, Bye, #[catch_all] Other(u5) }
//!
//! #[bitflags(u8)]
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Flags { urgent: bool, signed: bool, compressed: bool }
//!
//! // 5-bit Kind + a 3-bit slice of Flags would overflow a u8 together with Kind,
//! // so pack Kind (5) with three explicit bools (3) — exactly 8 bits.
//! #[bitfield(u8, bits = msb)]
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! struct Tag { kind: Kind, urgent: bool, signed: bool, compressed: bool }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Frame { tag: Tag, length: u16 }   // Tag (1 byte) + length (2 bytes)
//!
//! let frame = Frame {
//!     tag: Tag::new().with_kind(Kind::Data).with_urgent(true),
//!     length: 512,
//! };
//! let bytes = frame.to_bytes().unwrap();
//! assert_eq!(bytes.len(), 3);
//! assert_eq!(Frame::decode_exact(&bytes).unwrap(), frame);
//! ```
//!
//! # Widths are compiler-checked, never guessed
//!
//! A `#[bitfield]` can't know the bit width of a field whose type is another bitfield
//! or enum — that lives in `<T as Bits>::BITS`. So instead of computing offsets, the
//! macro emits **const expressions** (`<T as Bits>::BITS`, cumulative sums, masks) that
//! the compiler evaluates. Invalid layouts are rejected at compile time — for example
//! a reversed manual range is a clear error, not a silent miscompile:
//!
//! ```compile_fail
//! use bnb::bitfield;
//! #[bitfield(u16, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct Bad { #[bits(15..=0)] x: bnb::u16 } // reversed range (write it low..=high)
//! # let _ = Bad::new();
//! ```
//!
//! # Fixed vs. variable length: `FixedBitLen`
//!
//! A message with no variable-length field has a compile-time-constant encoded width
//! and implements [`FixedBitLen`](crate::FixedBitLen). That is what lets a fixed
//! message be embedded as a sized region inside another. A `count`-driven `Vec` (or a
//! `ctx`/`if`/positioning field) makes a message variable-length, so it implements the
//! codec traits but **not** `FixedBitLen`:
//!
//! ```
//! use bnb::{bin, FixedBitLen};
//!
//! #[bin(big)]
//! #[derive(Debug)]
//! struct Fixed { a: u16, b: u8 }       // always 24 bits
//!
//! assert_eq!(<Fixed as FixedBitLen>::BIT_LEN, 24);
//! ```
//!
//! See [`numbers`](super::numbers) for the `Bits` contract at the bottom of the stack,
//! and [`bitfields`](super::bitfields)/[`bin_codec`](super::bin_codec) for the layers
//! above it.
