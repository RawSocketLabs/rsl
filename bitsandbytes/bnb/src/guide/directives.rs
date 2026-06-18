//! Field-directive reference — `#[br]` (read), `#[bw]` (write), and the standalone
//! field attributes, one runnable example each.
//!
//! These appear on the fields of a [`#[bin]`](super::bin_codec) struct (or a bare
//! `#[derive(BitDecode/BitEncode)]`). Struct-level options (`magic`, `ctx`, `validate`,
//! `big`/`little`, …) are covered in [`bin_codec`](super::bin_codec).
//!
//! # `count` — a length-driven `Vec`
//!
//! `#[br(count = <expr>)]` reads that many elements into a `Vec<T>`; the expression may
//! name an earlier field. On write, every element is emitted (the length is the
//! caller's to track — usually with `temp`+`calc`, below).
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct List { len: u8, #[br(count = len)] items: Vec<u16> }
//!
//! let v = List { len: 2, items: vec![0xAABB, 0xCCDD] };
//! assert_eq!(v.to_bytes().unwrap(), [0x02, 0xAA, 0xBB, 0xCC, 0xDD]);
//! assert_eq!(List::decode_exact(&[0x02, 0xAA, 0xBB, 0xCC, 0xDD]).unwrap(), v);
//! ```
//!
//! # `temp` + `calc` — derived, never stored
//!
//! `#[br(temp)]` reads a value into a local that later directives can use, but does not
//! store it; its `#[bw(calc = <expr>)]` recomputes it on write. Together they keep a
//! length/count from ever drifting from the data it describes.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Msg {
//!     #[br(temp)]
//!     #[bw(calc = self.items.len() as u8)]
//!     n: u8,
//!     #[br(count = n)]
//!     items: Vec<u8>,
//! }
//!
//! let m = Msg { items: vec![10, 20, 30] };   // no `n` field to set
//! assert_eq!(m.to_bytes().unwrap(), [0x03, 10, 20, 30]);
//! assert_eq!(Msg::decode_exact(&[0x02, 5, 6]).unwrap().items, vec![5, 6]);
//! ```
//!
//! # `if` — a conditional `Option`
//!
//! `#[br(if(<cond>))]` on an `Option<T>` reads `Some` when the condition (over earlier
//! fields) holds, else `None`. On write, the `Option`'s presence drives whether it is
//! emitted.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Opt { has_ext: u8, #[br(if(has_ext != 0))] ext: Option<u16> }
//!
//! let with = Opt { has_ext: 1, ext: Some(0xBEEF) };
//! let without = Opt { has_ext: 0, ext: None };
//! assert_eq!(with.to_bytes().unwrap(), [0x01, 0xBE, 0xEF]);
//! assert_eq!(without.to_bytes().unwrap(), [0x00]);
//! assert_eq!(Opt::decode_exact(&[0x01, 0xBE, 0xEF]).unwrap(), with);
//! assert_eq!(Opt::decode_exact(&[0x00]).unwrap(), without);
//! ```
//!
//! # `map` / `try_map` — transform the wire value
//!
//! `#[br(map = <f>)]` reads the wire value (its type inferred from `f`'s argument) and
//! maps it to the field type; `#[bw(map = <f>)]` is the inverse on write. Use them to
//! store a friendly type while the wire keeps a raw encoding.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Reading {
//!     // wire: u8 biased by 40; stored: signed °C.
//!     #[br(map = |raw: u8| raw as i16 - 40)]
//!     #[bw(map = |c: &i16| (*c + 40) as u8)]
//!     celsius: i16,
//! }
//!
//! let r = Reading { celsius: 10 };
//! assert_eq!(r.to_bytes().unwrap(), [0x32]);              // 10 + 40 = 50
//! assert_eq!(Reading::decode_exact(&[0x32]).unwrap(), r);
//! ```
//!
//! `try_map` is the fallible form — the converter returns a `Result`, and an error
//! becomes a decode error (no panic):
//!
//! ```
//! use bnb::bin;
//! #[bin(big, read_only)]
//! #[derive(Debug, PartialEq)]
//! struct Checked {
//!     #[br(try_map = |raw: u8| if raw < 100 { Ok(raw) } else { Err("out of range") })]
//!     pct: u8,
//! }
//! assert_eq!(Checked::decode_exact(&[42]).unwrap().pct, 42);
//! assert!(Checked::decode_exact(&[200]).is_err());        // converter rejected it
//! ```
//!
//! # `parse_with` / `write_with` — a custom codec escape hatch
//!
//! When a field's shape needs arbitrary logic, supply your own functions. `parse_with`
//! takes `fn(&mut impl Source) -> Result<T, BitError>`; `write_with` takes
//! `fn(&T, &mut impl Sink) -> Result<(), BitError>`.
//!
//! ```
//! use bnb::{bin, BitError, Sink, Source};
//!
//! fn read_pascal<S: Source>(r: &mut S) -> Result<String, BitError> {
//!     let len: u8 = r.read()?;
//!     let mut s = String::new();
//!     for _ in 0..len { s.push(r.read::<u8>()? as char); }
//!     Ok(s)
//! }
//! fn write_pascal<K: Sink>(s: &String, w: &mut K) -> Result<(), BitError> {
//!     w.write(s.len() as u8)?;
//!     for &b in s.as_bytes() { w.write(b)?; }
//!     Ok(())
//! }
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Named {
//!     #[br(parse_with = read_pascal)]
//!     #[bw(write_with = write_pascal)]
//!     name: String,
//! }
//!
//! let n = Named { name: "Hi".into() };
//! assert_eq!(n.to_bytes().unwrap(), [0x02, b'H', b'i']);
//! assert_eq!(Named::decode_exact(&[0x02, b'H', b'i']).unwrap(), n);
//! ```
//!
//! # `brw(ignore)` — a field neither read nor written
//!
//! `#[brw(ignore)]` consumes no wire bits: the field is `Default::default()` on read and
//! skipped on write. Use it for derived/scratch state you want on the struct but not on
//! the wire. It is spelled with `brw` (not `br`) because it applies to **both**
//! directions.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Parsed { raw: u8, #[brw(ignore)] note: u32 }
//!
//! let p = Parsed { raw: 7, note: 999 };
//! assert_eq!(p.to_bytes().unwrap(), [0x07]);             // note not written
//! assert_eq!(Parsed::decode_exact(&[0x07]).unwrap(), Parsed { raw: 7, note: 0 });
//! ```
//!
//! # `reserved` / `reserved_with` — fixed wire bits with a spec value
//!
//! A reserved field is a normal **stored** field with a known *spec value*: the type's
//! zero for `#[reserved]`, the given expression for `#[reserved_with(<expr>)]` (e.g. a
//! must-be-one pattern). On the default path it reads and writes its *actual* value —
//! so you can observe a peer's reserved bits and override them — while the builder
//! defaults it to the spec value (so it isn't required) and the `spec_*` codecs use the
//! spec value instead.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct R {
//!     a: u8,
//!     #[reserved] pad: u8,                 // spec value 0x00
//!     #[reserved_with(0xFFu8)] ones: u8,   // spec value 0xFF
//!     b: u8,
//! }
//!
//! // The builder makes the reserved fields optional, defaulting to their spec values.
//! let r = R::builder().a(1).b(2).build().unwrap();
//! assert_eq!(r.to_bytes().unwrap(), [0x01, 0x00, 0xFF, 0x02]);
//!
//! // Decode captures the actual reserved bits; spec_decode reports the expected ones.
//! let actual = R::decode_exact(&[0x01, 0x55, 0x55, 0x02]).unwrap();
//! assert_eq!((actual.pad, actual.ones), (0x55, 0x55));
//! let spec = R::spec_decode_exact(&[0x01, 0x55, 0x55, 0x02]).unwrap();
//! assert_eq!((spec.pad, spec.ones), (0x00, 0xFF));     // the spec values
//! assert_eq!(spec.to_spec_bytes().unwrap(), [0x01, 0x00, 0xFF, 0x02]);
//! ```
//!
//! # `pad_*` / `align_*` — forward positioning
//!
//! `#[br(pad_before = <bits>)]` / `pad_after` skip a bit count around a field;
//! `align_before` / `align_after` skip to the next byte boundary. Bit/byte amounts come
//! from the [`prelude`](crate::prelude) (`1.bytes()`, `4.bits()`).
//!
//! ```
//! use bnb::{bin, prelude::*};
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct P { a: u8, #[br(pad_before = 1u32.bytes())] b: u8 }
//!
//! let p = P { a: 1, b: 2 };
//! assert_eq!(p.to_bytes().unwrap(), [0x01, 0x00, 0x02]); // one zero pad byte
//! assert_eq!(P::decode_exact(&[0x01, 0x99, 0x02]).unwrap(), p); // pad skipped on read
//! ```
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct A { flag: bool, #[br(align_before)] val: u8 }  // val starts on a byte boundary
//!
//! let a = A { flag: true, val: 0x2A };
//! assert_eq!(a.to_bytes().unwrap(), [0x80, 0x2A]); // flag in the high bit, then val
//! assert_eq!(A::decode_exact(&[0x80, 0x2A]).unwrap(), a);
//! ```
//!
//! # `restore_position` — peek without consuming
//!
//! `#[br(restore_position)]` reads the field, then rewinds the cursor so later fields
//! re-read the same bytes (e.g. peek a discriminant, then read the full record). The
//! field is not re-emitted on write — the overlapping field owns those bytes. It needs
//! a seekable source, so `decode_from` on a forward-only stream is a compile error; the
//! slice paths (`decode`/`peek`/`decode_exact`) always qualify.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Peeked {
//!     #[br(restore_position)] tag: u8, // peek the first byte...
//!     full: u16,                       // ...then read it as the high byte of a u16
//! }
//!
//! let p = Peeked::decode_exact(&[0xAB, 0xCD]).unwrap();
//! assert_eq!(p.tag, 0xAB);
//! assert_eq!(p.full, 0xABCD);
//! assert_eq!(p.to_bytes().unwrap(), [0xAB, 0xCD]); // `full` emits the bytes; `tag` does not
//! ```
