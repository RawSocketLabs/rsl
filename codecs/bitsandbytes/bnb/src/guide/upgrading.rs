//! Upgrading between `bnb` releases — what changes in your code, newest first.
//!
//! Each section lists what stops compiling, what compiles but behaves differently, and
//! what to write instead. Additions that need no action are at the end of the section.
//! The reasons and measurements behind each change are in the crate's `DESIGN.md`.
//!
//! # 0.9 → 0.10
//!
//! ## Changes the compiler tells you about
//!
//! **A writer's scratch must be `Send`.** [`BitWriter::with_scratch`](crate::BitWriter::with_scratch)
//! takes `Box<dyn Any + Send>`, so that a [`BitWriter`](crate::BitWriter) can move to
//! another thread or task. Make the scratch type `Send` (replace an `Rc` with an `Arc`,
//! a `RefCell` with a `Mutex`).
//!
//! **Field directives that cannot both take effect are rejected.** Before, one of the two
//! was silently dropped. The error names the pair and the rewrite; the common ones:
//!
//! | Rejected | Write instead |
//! |---|---|
//! | `if` with `map`, `try_map`, `parse_with` | put the conversion on the inner type; keep `if` on `Option<Inner>` |
//! | `if` with `count` | a plain `Vec` with `count = if cond { n } else { 0 }` |
//! | `if` with `ctx` | read the child in a `parse_with` function that takes the condition |
//! | `count` or `ctx` with `map`, `try_map`, `parse_with` | size the read inside `parse_with`, or read a `temp` field and derive with `br(calc)` |
//! | `bw(calc)` with `bw(map)`, `write_with`, `auto_len` | compute the wire value in `calc`; drop `calc` beside `auto_len` |
//! | `auto_len` with `bw(map)`, `write_with`, `temp` | a plain stored `WireLen<T>` field |
//! | `restore_position` with `bw(map)`, `write_with`, `auto_len`, or on a `temp` field with `bw(calc)` | remove the write-side directive: a peek is never written |
//! | `ignore` or `reserved` with most other directives | keep one |
//! | the same directive twice with different values | keep one (`assert` may repeat) |
//!
//! **A stored `#[bw(calc)]` on an enum variant field is rejected.** An enum has no
//! canonical encode, so the expression would never run. Make the field `temp` (it is then
//! recomputed on every encode), or move the variant's fields into a `#[bin]` struct:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! enum Frame {
//!     #[bin(magic = 1u8)]
//!     Data {
//!         #[br(temp)]
//!         #[bw(calc = u8::try_from(body.len()).unwrap())]
//!         len: u8,
//!         #[br(count = len)]
//!         body: Vec<u8>,
//!     },
//! }
//!
//! let frame = Frame::Data { body: vec![7, 8] };
//! assert_eq!(frame.to_bytes().unwrap(), [1, 2, 7, 8]);
//! ```
//!
//! **Container options that were ignored are rejected:** `auto_len(...)` on an enum (put
//! `#[bw(auto_len = …)]` on the variant's `WireLen` field); `big`, `little`, `bytes`,
//! `bits`, `auto_len` and `forward_only` on a wire-mapped struct (they belong on the wire
//! type); `auto_len` and `forward_only` on a `codec` newtype.
//!
//! **Field names that collide with generated items are rejected** with an error on the
//! field, in `#[bitfield]`, `#[bitflags]` and builders (for example a field `new`, or
//! `x` beside `with_x`). They used to fail inside generated code. Rename the field.
//!
//! ## Changes the compiler does not tell you about
//!
//! **`UInt::try_from` evaluates an untyped argument as `i32`.** `u1`..`u127` now convert
//! from every primitive integer, so `u12::try_from(len)` works for a `usize` with no cast.
//! The price is that an integer expression whose type nothing else fixes is no longer
//! inferred to be the backing type: it is a signed `i32`, as in `u64::try_from(..)`. A
//! small literal is unaffected, but `!`, `>>`, `/`, `%` and bit 31 mean something else on
//! a signed value. Give one operand a type:
//!
//! ```
//! use bnb::u12;
//!
//! // 0.9: `u12::try_from(!0 >> 4)` was `Ok(4095)`. In 0.10 the untyped `!0` is `-1_i32`.
//! assert!(u12::try_from(!0 >> 4).is_err());
//! assert_eq!(u12::try_from(!0_u16 >> 4).map(u12::value), Ok(4095));
//!
//! // No cast needed for a length, and nothing is truncated.
//! let len: usize = 65_541;
//! assert!(u12::try_from(len).is_err());
//! ```
//!
//! A bare literal of 2³¹ or more (`u48::try_from(0x1_0000_0000)`) is caught by the
//! compiler's `overflowing_literals` lint: suffix it (`0x1_0000_0000_u64`).
//!
//! **`decode_all` and `decode_iter` report a message that does not advance.** A message
//! that decodes without ending past where it started (a zero-width type, or a backward
//! `seek`) is [`ErrorKind::NoProgress`](crate::ErrorKind::NoProgress). A backward seek used
//! to loop forever; a zero-width type used to end the walk silently, dropping the unread
//! bytes.
//!
//! **`auto_len = bytes(x)` measures `x` as it is written.** The length now honours the
//! source field's own `write_with`, `bw(map)`, `ctx` and `count_prefix`. If a message
//! relied on the old measure (a bare encode of the field's type), its length field changes
//! on the wire; in every case found the old value was simply wrong for the bytes that
//! followed.
//!
//! **Enum variant fields encode like struct fields.** A `#[br(calc)]` variant field is no
//! longer written, `auto_len` resolves on a variant, and `ignore` takes precedence as it
//! does on a struct.
//!
//! **A long-lived `BufSource` (`std`) needs `compact()`.** Its cap bounds the
//! bytes it retains, and it retains every byte until told otherwise, so a loop decoding
//! message after message ends in `BufferFull` once `cap` bytes have gone by. That was
//! already true in 0.9 and is now documented, with the way out:
//!
//! ```
//! use bnb::{BufSource, Source};
//!
//! let wire = [0x12, 0x34, 0x56, 0x78];
//! let mut src = BufSource::with_capacity(&wire[..], 2);
//! assert_eq!(src.read_bits(16).unwrap(), 0x1234);
//! src.compact(); // done with that message: release its bytes
//! assert_eq!(src.read_bits(16).unwrap(), 0x5678);
//! assert_eq!(src.bit_pos(), 32); // positions stay stream-absolute
//! ```
//!
//! After `compact()`, a seek to a position before the retained bytes is
//! [`ErrorKind::NotSeekable`](crate::ErrorKind::NotSeekable). A read interrupted by a
//! signal is retried instead of failing the decode.
//!
//! **Canonical encode is per message.** `to_canonical_bytes`, `to_canonical` and
//! `is_canonical` cover the message's own `reserved` and `calc` fields; a nested message
//! or `Vec` element is written as stored. This is unchanged behaviour, but the docs used
//! to promise more. Canonicalize a child first if it has such fields of its own.
//!
//! ## Additions
//!
//! - `UInt` implements `LowerHex`, `UpperHex`, `Octal` and `Binary`.
//! - [`WidthError`](crate::WidthError) has a `Negative` variant, for a negative source.
//! - `BufSource::compact`.
//! - Variants of a `#[bin]` enum may share a `magic`: `decode` takes the first declared
//!   match and the others are reached with `decode_as_<variant>`.
//! - A `read_only` or `write_only` type no longer needs anything for the other direction.
//! - `bool`, `u8` and other field types written through a `macro_rules!` type fragment
//!   are recognised.
//!
//! ## Performance
//!
//! No action needed. Unaligned and sub-byte fields decode two to seven times faster,
//! byte-aligned decode about a quarter faster, a fixed-length message encodes with one
//! allocation, and byte-aligned `Vec<u16>`/`u32`/`u64`/`u128` runs convert in one pass. A
//! `MessageStream` now holds a 4 KiB read scratch for its whole life.
