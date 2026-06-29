//! Whole-struct wire mapping — `#[bin(map = …, bw_map = …)]`.
//!
//! [`directives`](super::directives) showed `#[br(map)]`/`#[bw(map)]` transforming one *field*
//! between its wire value and a friendlier type. The same idea applies to a **whole struct**: a
//! *logical* type whose on-the-wire form is a different *wire* type. `#[bin]` on the logical type
//! takes the mapping and generates the codec by delegating to the wire type.
//!
//! - **`map = |w: Wire| Logical`** (decode): read the wire message, then map it to the logical
//!   type. The wire type is taken from the closure's annotated parameter.
//! - **`bw_map = |l: &Logical| Wire`** (encode): map the logical value to its wire form, then
//!   write that.
//! - **`try_map = |w: Wire| Result<Logical, E>`**: the fallible decode form — a conversion error
//!   becomes a decode error ([`ErrorKind::Convert`](crate::ErrorKind)); the parser never panics.
//!
//! The struct's own fields are the **logical** data — never read or written directly; the wire
//! type owns the bytes.
//!
//! # A logical type over a packed wire form
//!
//! ```
//! use bnb::bin;
//!
//! // The wire form: a normal #[bin] message. Coordinates are stored biased by 128
//! // (mapping a signed range onto an unsigned byte).
//! #[bin(big)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct WirePoint { x_biased: u8, y_biased: u8 }
//!
//! // The logical form: signed coordinates, mapped to/from WirePoint.
//! #[bin(
//!     map = |w: WirePoint| Point { x: w.x_biased as i16 - 128, y: w.y_biased as i16 - 128 },
//!     bw_map = |p: &Point| WirePoint { x_biased: (p.x + 128) as u8, y_biased: (p.y + 128) as u8 }
//! )]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Point { x: i16, y: i16 }
//!
//! let p = Point { x: -10, y: 20 };
//! assert_eq!(p.to_bytes().unwrap(), [118, 148]);            // mapped onto the wire
//! assert_eq!(Point::decode_exact(&[118, 148]).unwrap(), p); // and back
//! ```
//!
//! # The generated surface
//!
//! A mapped type gets the same entry points as any `#[bin]` message — `decode`, `decode_exact`,
//! `decode_all`, `decode_iter`, `peek`, `to_bytes` — plus the [`BitDecode`](crate::BitDecode) /
//! [`BitEncode`](crate::BitEncode) impls, so **it nests as a field** in another `#[bin]` (it
//! forwards [`FixedBitLen`](crate::FixedBitLen) from the wire type, sizing a fixed region):
//!
//! ```
//! use bnb::bin;
//! # #[bin(big)] #[derive(Debug, Clone, PartialEq)] struct WirePoint { x_biased: u8, y_biased: u8 }
//! # #[bin(map = |w: WirePoint| Point { x: w.x_biased as i16 - 128, y: w.y_biased as i16 - 128 },
//! #       bw_map = |p: &Point| WirePoint { x_biased: (p.x + 128) as u8, y_biased: (p.y + 128) as u8 })]
//! # #[derive(Debug, Clone, PartialEq)] struct Point { x: i16, y: i16 }
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Frame { tag: u8, p: Point }
//!
//! let f = Frame { tag: 7, p: Point { x: 1, y: 2 } };
//! assert_eq!(f.to_bytes().unwrap(), [7, 129, 130]);
//! assert_eq!(Frame::decode_exact(&[7, 129, 130]).unwrap(), f);
//! ```
//!
//! # Fallible mapping
//!
//! ```
//! use bnb::bin;
//!
//! // The wire form is a bare `u8`; the logical type rejects values over 100 on read.
//! #[bin(
//!     try_map = |w: u8| if w <= 100 { Ok(Pct(w)) } else { Err("percent over 100") },
//!     bw_map = |p: &Pct| p.0
//! )]
//! #[derive(Debug, PartialEq)]
//! struct Pct(u8);
//!
//! assert_eq!(Pct::decode_exact(&[42]).unwrap(), Pct(42));
//! assert!(Pct::decode_exact(&[200]).is_err()); // the converter rejected it (no panic)
//! ```
//!
//! # Notes
//!
//! - **One direction is fine.** Give only `map`/`try_map` for a decode-only type, or only
//!   `bw_map` with an annotated return (`|s: &T| -> Wire { … }`) for an encode-only one.
//! - **The wire type must be fixed-length** (the mapped type forwards `FixedBitLen`). For a
//!   variable-length wire form, hand-write [`BitDecode`](crate::BitDecode) /
//!   [`BitEncode`](crate::BitEncode) — this attribute is just the sugar over exactly that.
//! - **Byte/bit order comes from the wire type**, so `big`/`little`/`bit_order` on the mapped
//!   struct itself don't apply, and `magic`/`ctx`/`validate` belong on the wire type.
