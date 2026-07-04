//! Whole-struct wire mapping — a *logical* type that serializes via a separate *wire* type.
//!
//! [`directives`](super::directives) showed `#[br(map)]`/`#[bw(map)]` transforming one *field*
//! between its wire value and a friendlier type. The same idea applies to a **whole struct**: a
//! *logical* type whose on-the-wire form is a different *wire* type. The struct's own fields are
//! the logical data — never read or written directly; the wire type owns the bytes.
//!
//! There are two forms:
//!
//! - **Conversion-trait form** — `#[bin(wire = WireType)]`, driven by standard `From`/`TryFrom`
//!   impls you write. The transitions live in named `impl` blocks — a clean home, reusable
//!   anywhere in your program — and the **wire type may be variable-length**.
//! - **Closure form** — `#[bin(map = …, bw_map = …)]`, with the transitions inline in the
//!   attribute. Quick for a small fixed mapping.
//!
//! # The conversion-trait form
//!
//! `#[bin(wire = WireType)]` needs `From<WireType> for Self` (decode) and `From<&Self> for
//! WireType` (encode). Decode reads `WireType` then `Self::from(wire)`; encode does
//! `WireType::from(&self)` then writes.
//!
//! ```
//! use bnb::bin;
//!
//! // The wire form: a normal #[bin] message.
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct WireColor { r: u8, g: u8, b: u8 }
//!
//! // The logical form: a packed u32, mapped to/from WireColor.
//! #[bin(wire = WireColor)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Color { rgb: u32 }
//!
//! impl From<WireColor> for Color {
//!     fn from(w: WireColor) -> Self {
//!         Color { rgb: (w.r as u32) << 16 | (w.g as u32) << 8 | w.b as u32 }
//!     }
//! }
//! impl From<&Color> for WireColor {
//!     fn from(c: &Color) -> Self {
//!         WireColor { r: (c.rgb >> 16) as u8, g: (c.rgb >> 8) as u8, b: c.rgb as u8 }
//!     }
//! }
//!
//! let c = Color { rgb: 0x11_22_33 };
//! assert_eq!(c.to_bytes().unwrap(), [0x11, 0x22, 0x33]);            // mapped onto the wire
//! assert_eq!(Color::decode_exact(&[0x11, 0x22, 0x33]).unwrap(), c); // and back
//! let w: WireColor = (&c).into();                                  // …and reusable in-program
//! assert_eq!(w.r, 0x11);
//! ```
//!
//! Use `#[bin(try_wire = WireType)]` for a fallible decode (`TryFrom<WireType> for Self`, whose
//! `Error: Display`); a conversion error becomes a decode error
//! ([`ErrorKind::Convert`](crate::ErrorKind)) — the parser never panics.
//!
//! # The closure form
//!
//! When the mapping is small and you don't need a reusable `From`, put it inline:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct WireTemp { biased: u8 } // stored as celsius + 40
//!
//! #[bin(
//!     map = |w: WireTemp| Celsius(w.biased as i16 - 40),
//!     bw_map = |c: &Celsius| WireTemp { biased: (c.0 + 40) as u8 }
//! )]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Celsius(i16);
//!
//! assert_eq!(Celsius(10).to_bytes().unwrap(), [50]);
//! assert_eq!(Celsius::decode_exact(&[50]).unwrap(), Celsius(10));
//! ```
//!
//! `try_map = |w: Wire| Result<Self, E>` is the fallible closure form (the dual of `try_wire`).
//!
//! # Variable-length logical formats
//!
//! Because a mapped type does **not** auto-implement [`FixedBitLen`](crate::FixedBitLen), its
//! wire form may be variable-length — a count-driven `Vec`, a length-prefixed string, anything:
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct WireText { n: u8, #[br(count = n)] data: Vec<u8> } // variable-length
//!
//! #[bin(wire = WireText)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Text(String);
//! impl From<WireText> for Text {
//!     fn from(w: WireText) -> Self { Text(String::from_utf8_lossy(&w.data).into_owned()) }
//! }
//! impl From<&Text> for WireText {
//!     fn from(t: &Text) -> Self { WireText { n: t.0.len() as u8, data: t.0.as_bytes().to_vec() } }
//! }
//!
//! let t = Text("hi".into());
//! assert_eq!(t.to_bytes().unwrap(), [2, b'h', b'i']);
//! assert_eq!(Text::decode_exact(&[3, b'a', b'b', b'c']).unwrap(), Text("abc".into()));
//! ```
//!
//! # Generated surface and nesting
//!
//! A mapped type gets the same entry points as any `#[bin]` message — `decode`, `decode_exact`,
//! `decode_all`, `decode_iter`, `peek`, `to_bytes` — plus the [`BitDecode`](crate::BitDecode) /
//! [`BitEncode`](crate::BitEncode) impls. It nests as a field via a `count`/`ctx`/`if` directive
//! like any message; to nest a **fixed-wire** mapped type as a *plain* field, add a one-line
//! [`FixedBitLen`](crate::FixedBitLen) impl forwarding the wire type's size (it only compiles if
//! the wire really is fixed-length):
//!
//! ```
//! use bnb::bin;
//! # #[bin(big)] #[derive(Debug, Clone, PartialEq)] struct WireColor { r: u8, g: u8, b: u8 }
//! # #[bin(wire = WireColor)] #[derive(Debug, Clone, PartialEq)] struct Color { rgb: u32 }
//! # impl From<WireColor> for Color { fn from(w: WireColor) -> Self { Color { rgb: (w.r as u32) << 16 | (w.g as u32) << 8 | w.b as u32 } } }
//! # impl From<&Color> for WireColor { fn from(c: &Color) -> Self { WireColor { r: (c.rgb >> 16) as u8, g: (c.rgb >> 8) as u8, b: c.rgb as u8 } } }
//! impl bnb::FixedBitLen for Color {
//!     const BIT_LEN: u32 = <WireColor as bnb::FixedBitLen>::BIT_LEN;
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Pixel { color: Color, alpha: u8 }
//!
//! let px = Pixel { color: Color { rgb: 0x01_02_03 }, alpha: 0xFF };
//! assert_eq!(px.to_bytes().unwrap(), [0x01, 0x02, 0x03, 0xFF]);
//! assert_eq!(Pixel::decode_exact(&[0x01, 0x02, 0x03, 0xFF]).unwrap(), px);
//! ```
//!
//! # Notes
//!
//! - **One direction is fine.** For the closure form, give only `map`/`try_map` (decode-only) or
//!   only `bw_map` (encode-only). For the conversion-trait form, add `read_only`/`write_only`.
//! - **Byte/bit order comes from the wire type**, so `big`/`little`/`bit_order` on the mapped
//!   struct itself don't apply, and `magic`/`ctx`/`validate` belong on the wire type.
//! - **Pick a form:** reach for `wire`/`try_wire` when you want the transitions in a named,
//!   reusable place or the wire form is variable-length; reach for `map`/`bw_map` for a quick
//!   inline fixed mapping.
