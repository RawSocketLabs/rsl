//! Arbitrary-width integers and the [`Bits`](crate::Bits) trait.
//!
//! # `u1`..`u127`
//!
//! Sub-byte fields need integers narrower than `u8`, and odd widths like 12 or 108
//! bits. `bnb` provides them as type aliases over [`UInt`](crate::UInt): `u4` is
//! `UInt<u8, 4>`, `u12` is `UInt<u16, 12>`, `u108` is `UInt<u128, 108>`, and so on —
//! each backed by the smallest primitive that holds it. The native widths (`u8`,
//! `u16`, `u32`, `u64`, `u128`) are the standard library's and need no wrapper.
//!
//! A value is always in range `0..=MAX`:
//!
//! ```
//! use bnb::u5;
//!
//! assert_eq!(u5::MAX.value(), 31);     // 2^5 - 1
//! assert_eq!(u5::MIN.value(), 0);
//! let x = u5::new(17);                 // checked: panics if > 31
//! assert_eq!(x.value(), 17);
//! ```
//!
//! ## Constructing values
//!
//! Pick the constructor by how you want out-of-range input handled:
//!
//! ```
//! use bnb::u4;
//!
//! let a = u4::new(0xA);                  // panics on overflow — for known-good consts
//! let b = u4::try_new(0x10);             // Result — for untrusted input
//! assert!(b.is_err());
//! let c = u4::from_raw(0xFF);            // masks to the low 4 bits — never fails
//! assert_eq!(c.value(), 0xF);
//! assert_eq!(u4::default().value(), 0);  // zero
//! ```
//!
//! ## Converting to and from the backing primitive
//!
//! ```
//! use bnb::u12;
//!
//! let v = u12::new(0xABC);
//! let raw: u16 = v.into();               // widening is infallible
//! assert_eq!(raw, 0xABC);
//! assert_eq!(u12::try_from(0xABCu16).unwrap(), v); // narrowing is checked
//! assert!(u12::try_from(0x1000u16).is_err());
//! ```
//!
//! # The `Bits` trait — why everything composes
//!
//! [`Bits`](crate::Bits) is the one interface the whole crate is built on: a value
//! that occupies a fixed number of bits. Its contract is just two methods around a
//! `u128` carrier — `into_bits` (the value in the low `BITS` bits) and `from_bits`
//! (reconstruct from the low `BITS` bits) — plus the `BITS` width:
//!
//! ```
//! use bnb::{Bits, u4};
//!
//! assert_eq!(<u4 as Bits>::BITS, 4);
//! assert_eq!(<bool as Bits>::BITS, 1);
//! assert_eq!(<u32 as Bits>::BITS, 32);
//!
//! // The round-trip law every impl upholds: from_bits(x.into_bits()) == x.
//! let x = u4::new(0xD);
//! assert_eq!(u4::from_bits(x.into_bits()), x);
//!
//! // from_bits reads only the low BITS bits; higher bits are ignored.
//! assert_eq!(u8::from_bits(0x1FF), 0xFF);
//! assert!(!bool::from_bits(0b10));
//! ```
//!
//! `bool`, the primitive integers, and every `UInt` implement `Bits` out of the box;
//! `#[bitfield]`, `#[derive(BitEnum)]`, and `#[bitflags]` generate impls so those
//! types nest as fields too. That single abstraction is what lets a `u5` enum sit
//! inside a `u16` bitfield inside a byte-aligned `#[bin]` message — see
//! [`composition`](super::composition).
