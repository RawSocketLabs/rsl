//! `#[derive(BitEnum)]` — map an enum to a fixed-width integer discriminant.
//!
//! ```
//! use bnb::{BitEnum, Bits, u4};
//!
//! #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u4)]
//! enum Op {
//!     Read,    // 0 (auto-numbered from 0)
//!     Write,   // 1
//!     Erase,   // 2
//!     #[catch_all]
//!     Other(u4),
//! }
//!
//! assert_eq!(Op::Write.into_bits(), 1);
//! assert_eq!(Op::from_bits(2), Op::Erase);
//! ```
//!
//! `#[bit_enum(uN)]` sets the width. Unit variants take auto-incrementing
//! discriminants from 0, or you can pin them with `= N`:
//!
//! ```
//! use bnb::Bits;
//!
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u8)]
//! #[repr(u8)]
//! enum EtherType {
//!     Ipv4 = 0x00,
//!     Arp = 0x06,
//!     #[catch_all]
//!     Other(u8),
//! }
//! assert_eq!(EtherType::Arp.into_bits(), 0x06);
//! ```
//!
//! (Rust forbids explicit discriminants on an enum that also has a tuple variant
//! unless it carries `#[repr(..)]`; for contiguous-from-0 values, drop the `= N` and
//! the `#[repr]`.)
//!
//! # Catch-all: lossless, dual-use decoding
//!
//! A single `#[catch_all]` tuple variant (holding the width type) captures any value
//! the named variants don't, so decoding is **total and lossless** — the parser never
//! rejects representable input, and the original value round-trips:
//!
//! ```
//! use bnb::{BitEnum, Bits, u4};
//! # #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! # #[bit_enum(u4)]
//! # enum Op { Read, Write, Erase, #[catch_all] Other(u4) }
//! assert_eq!(Op::from_bits(9), Op::Other(u4::new(9))); // unknown — preserved
//! assert_eq!(Op::Other(u4::new(9)).into_bits(), 9);     // round-trips exactly
//! ```
//!
//! # No catch-all: exhaustive, or `closed`
//!
//! Without a catch-all, `from_bits` (the infallible decode path) has nowhere to put
//! an unknown discriminant. So the derive requires one of two things:
//!
//! - the variants **cover the whole width** (then an unknown value is impossible), or
//! - you mark the enum **`closed`** to assert it is a closed set on purpose.
//!
//! A fully-covered enum needs no marker:
//!
//! ```
//! use bnb::{Bits, u2};
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u2)]
//! enum Quadrant { Ne, Nw, Sw, Se }   // all 4 values of a u2 named — exhaustive
//! assert_eq!(Quadrant::from_bits(3), Quadrant::Se);
//! ```
//!
//! An enum that is *not* exhaustive and has no catch-all is a **compile error** unless
//! marked `closed` — the diagnostic points you at both fixes:
//!
//! ```compile_fail
//! #[derive(bnb::BitEnum, Clone, Copy)]
//! #[bit_enum(u8)]
//! enum Bad { A = 1, B = 2 }   // error: not exhaustive, no #[catch_all], not `closed`
//! # let _ = Bad::A;
//! ```
//!
//! With `closed` it compiles; the checked `TryFrom` rejects unknowns, while the
//! infallible `from_bits` **panics** on an out-of-set value (a declared contract
//! violation), so only use `closed` for sets you never decode from untrusted bytes:
//!
//! ```should_panic
//! use bnb::Bits;
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u8, closed)]
//! #[repr(u8)]
//! enum Direction { Request = 1, Reply = 2 }
//!
//! assert_eq!(Direction::try_from(2u8), Ok(Direction::Reply)); // checked: fine
//! assert!(Direction::try_from(7u8).is_err());                 // checked: rejected
//! let _ = Direction::from_bits(7);                            // infallible: panics
//! ```
//!
//! # `num_enum` parity for byte-aligned enums
//!
//! When the width is a whole-byte primitive (`u8`/`u16`/…), the derive also emits the
//! `num_enum`-style primitive conversions — so a magic-byte enum needs no hand-written
//! `From` impl or round-trip test:
//!
//! - `From<Enum> for uN` — always;
//! - with a catch-all, `From<uN> for Enum` — total (unknowns absorbed);
//! - without one (a `closed` enum), `TryFrom<uN> for Enum` — checked, erroring with
//!   [`UnknownDiscriminant`](crate::UnknownDiscriminant).
//!
//! ```
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u8)] #[repr(u8)]
//! enum EtherType { Ipv4 = 0x00, Arp = 0x06, #[catch_all] Other(u8) }
//!
//! assert_eq!(u8::from(EtherType::Arp), 0x06);             // enum -> primitive
//! assert_eq!(EtherType::from(0x06u8), EtherType::Arp);    // total (has catch-all)
//! assert_eq!(EtherType::from(0x99u8), EtherType::Other(0x99));
//! ```
//!
//! A sub-byte enum (`u4`) gets only the [`Bits`](crate::Bits)/[`BitEnum`](crate::BitEnum)
//! impls — it is meaningful only nested in a [`#[bitfield]`](super::bitfields) or a
//! `#[bin]` message, where its 4 bits are placed in context.
