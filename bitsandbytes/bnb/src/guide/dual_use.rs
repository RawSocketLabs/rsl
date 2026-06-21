//! Dual-use: compliant by default, deliberately violatable.
//!
//! These types are meant for both sides of the wire: the guided path emits and accepts
//! RFC-correct traffic, but a caller who *wants* to produce or read non-conformant
//! bytes — for fuzzing, red-teaming, or interop testing — can. The rules that make
//! that work:
//!
//! 1. **Builder defaults are compliant**, but the fields stay settable.
//! 2. **Parsers accept representable-but-non-compliant values** — unknowns are modelled
//!    as data (`#[catch_all]`, retained flag bits), never hard errors.
//! 3. **Policy lives on the construction path, never in a parser.** `validate` gates
//!    `build()`; decoding stays permissive.
//! 4. **Raw constructors never validate** — they are the open escape hatch.
//!
//! # Unknowns are preserved, not rejected
//!
//! A `#[catch_all]` enum and a flag set both round-trip values they don't have a name
//! for, so a parser never throws away or rejects representable input:
//!
//! ```
//! use bnb::{BitEnum, Bits, bitflags, u4};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u4)]
//! enum Op { Read, Write, #[catch_all] Other(u4) }
//!
//! assert_eq!(Op::from_bits(0xE), Op::Other(u4::new(0xE))); // preserved...
//! assert_eq!(Op::Other(u4::new(0xE)).into_bits(), 0xE);     // ...and round-trips
//!
//! #[bitflags(u8)]
//! #[derive(Clone, Copy)]
//! struct F { a: bool, b: bool }
//! assert_eq!(F::from_bits(0b1011).bits(), 0b1011); // undefined bits retained
//! ```
//!
//! # The parser stays permissive; `validate` is construction-only
//!
//! `#[bin(validate = …)]` runs only in `build()`. Decoding the very same malformed
//! bytes still succeeds — so you can parse hostile input for analysis, but can't
//! *accidentally build* a malformed message:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, validate = check)]
//! #[derive(Debug, PartialEq)]
//! struct Cell { tag: u8 }
//!
//! fn check(c: &Cell) -> Result<(), String> {
//!     if c.tag > 3 { return Err("tag must be 0..=3".into()); }
//!     Ok(())
//! }
//!
//! assert!(Cell::builder().tag(9).build().is_err());  // construction: rejected
//! assert_eq!(Cell::decode_exact(&[9]).unwrap(), Cell { tag: 9 }); // parser: accepts
//! ```
//!
//! # Raw constructors are the escape hatch
//!
//! `from_raw` (on a `#[bitfield]`) and `from_bits` never validate — they let you build
//! any representable bit pattern on purpose:
//!
//! ```
//! use bnb::{bitfield, u4};
//! #[bitfield(u8, bits = msb)]
//! #[derive(Clone, Copy)]
//! struct B { hi: u4, lo: u4 }
//!
//! let weird = B::from_raw(0xFF);   // no checks — exactly the bits you asked for
//! assert_eq!(weird.hi().value(), 0xF);
//! ```
//!
//! # Encoding reproduces; normalizing is opt-in
//!
//! The same principle governs *output*. The default
//! [`to_bytes`](super::bin_codec#two-encode-forms-verbatim-vs-canonical) is **verbatim** — it
//! re-emits exactly what you hold (retained `reserved` bits, a stored `calc` value), so a
//! message you parsed off the wire round-trips byte-for-byte, malformed or not. Normalizing to
//! the spec is the *explicit* `to_canonical_bytes` (or setting the value's `encode_mode` to
//! `Canonical`, which `encode` then follows), never something the codec does behind your back.
//!
//! # What is still rejected
//!
//! Only the *physically unencodable* is refused — never the merely non-conformant. A
//! value that doesn't fit its field's bits can't be represented, so checked
//! construction errors (and `new` panics):
//!
//! ```
//! use bnb::u4;
//! assert!(u4::try_new(0x10).is_err()); // 16 doesn't fit in 4 bits — unencodable
//! ```
//!
//! The one place a *decode* can panic is a [`closed`](super::enums) enum fed an
//! out-of-set discriminant — which is exactly why `closed` is an explicit opt-in and
//! the default for untrusted input is `#[catch_all]`.
