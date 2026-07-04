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
//! caller's to track — usually with `temp`+`calc`, below, or the `count_prefix` sugar).
//!
//! **The count obligation.** The expression drives *decode* sizing only — encode trusts
//! the `Vec` and writes everything it holds. If a stored count (or a `ctx` param, which
//! doesn't even exist at encode time) disagrees with `len()`, the emitted bytes won't
//! round-trip. Keeping them consistent is the constructor's job: derive the count
//! (`count_prefix` / `temp`+`calc`) where the layout allows, or enforce it with
//! construction-side `validate` (see the `ctx_length` example). The flip side is
//! deliberate: a *stored* count field can disagree on purpose — that's how dual-use
//! code forges adversarial frames.
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
//! # `count_prefix` — the length-prefixed count, in one line
//!
//! A count immediately followed by the elements it counts is the most common shape of
//! the `temp`+`calc`+`count` triad above — so `#[brw(count_prefix = <Ty>)]` on the `Vec`
//! generates the whole triad. The prefix is read into a hidden local, sizes the `Vec`,
//! and is recomputed from `len()` on write: derived, never stored, can never drift.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Msg {
//!     #[brw(count_prefix = u8)]
//!     items: Vec<u8>,
//! }
//!
//! let m = Msg { items: vec![10, 20, 30] };
//! assert_eq!(m.to_bytes().unwrap(), [0x03, 10, 20, 30]);   // same wire as the triad
//! assert_eq!(Msg::decode_exact(&[0x02, 5, 6]).unwrap().items, vec![5, 6]);
//! ```
//!
//! Any [`Bits`](crate::Bits) type works as the prefix — a `uN` occupies its declared
//! width, so a `count_prefix = u12` puts a true 12-bit count on the wire. Encode is
//! **checked**: a collection too long for the prefix is a [`BitError`](crate::BitError)
//! (`Convert`), never a silently wrapped count (which is what a hand-written
//! `as u8` calc would do).
//!
//! ```
//! # use bnb::bin;
//! # #[bin(big)]
//! # #[derive(Debug, PartialEq)]
//! # struct Msg { #[brw(count_prefix = u8)] items: Vec<u8> }
//! let too_long = Msg { items: vec![0; 300] };            // 300 > u8::MAX
//! assert!(too_long.to_bytes().is_err());                 // checked, not truncated
//! ```
//!
//! `count_prefix` is *adjacent* (immediately before its `Vec`) and derive-only (never
//! stored, so never overridable). When the count sits **away from** its data (DNS's
//! `qdcount`…`arcount` header block) or measures **bytes** rather than elements (DNS's
//! `rdlength`), or when you want to *forge* a disagreeing length — reach for [`WireLen`],
//! below.
//!
//! # `WireLen` — an auto-deriving, overridable length/count
//!
//! [`WireLen<T>`](crate::WireLen) is a length field that is either `auto()` (derive at
//! encode — the default) or `set(n)` (an explicit value). Decode always yields `Set`, so a
//! plain `to_bytes()` is correct by default *and* `decode → encode` is byte-identical (a
//! forged length survives a round-trip). It's the dual-use, non-adjacent, byte-or-element
//! counterpart to `count_prefix`:
//!
//! - **Same-struct**: `#[bw(auto_len = count(<field>))]` (element count) or
//!   `#[bw(auto_len = bytes(<field>))]` (encoded byte length) on the `WireLen` field.
//! - **Cross-struct**: `#[bin(auto_len(<field>.<nested> = count(<source>), …))]` on the
//!   enclosing struct — a count nested in a sub-struct that sizes a sibling collection. The
//!   targeted sub-struct must be `Clone` (it is filled through a clone at encode), and an
//!   `auto_len` target may carry only positioning directives, no other codec directive.
//!
//! ```
//! use bnb::{bin, WireLen};
//!
//! #[bin(big)]
//! # #[derive(Debug, PartialEq)]
//! struct Framed {
//!     #[bw(auto_len = count(items))]      // Auto → items.len(); Set(n) → n (a forgery)
//!     len: WireLen<u16>,
//!     #[br(count = len.to_count())]
//!     items: Vec<u8>,
//! }
//!
//! let m = Framed { len: WireLen::auto(), items: vec![1, 2, 3] };
//! assert_eq!(m.to_bytes().unwrap(), [0, 3, 1, 2, 3]);      // auto-derived
//! let forged = Framed { len: WireLen::set(9), items: vec![1, 2, 3] };
//! assert_eq!(forged.to_bytes().unwrap(), [0, 9, 1, 2, 3]); // the lie is written
//! ```
//!
//! Derivation is **checked** (a too-long collection is a [`BitError`](crate::BitError), never
//! a truncation), and a `WireLen`-auto field is optional in the builder (defaults to
//! `auto()`), so you never mention it unless forging.
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
//! # `assert` — a decode-time guard
//!
//! `#[br(assert(<expr>))]` runs after the field is read (and mapped): if the expression —
//! over this and any earlier field — is false, decode fails with a position-aware
//! [`Convert`](crate::bitstream::ErrorKind::Convert) error naming the field. An optional
//! message takes format args: `assert(<expr>, "fmt {}", args…)`. Multiple asserts run in
//! order.
//!
//! **Doctrine note.** The parser is permissive by default — it never rejects representable
//! input. `assert` is the *explicit opt-in* for values that are unrepresentable in your
//! domain (an impossible version, a violated framing invariant) — the same rejection family
//! as `magic`, closed enums, and `try_map`. It is **read-only**: no `bw` inverse is needed,
//! and encode still writes whatever is stored — you can forge what you would not accept
//! (dual-use). For *semantic* validity, use struct-level `validate` (construction-side,
//! never the parser); for a genuine type conversion, use `try_map`.
//!
//! ```
//! use bnb::bin;
//! use bnb::bitstream::ErrorKind;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Event {
//!     #[br(assert((1..=2).contains(&version), "unsupported version {}", version))]
//!     version: u8,
//!     id: u16,
//! }
//!
//! assert!(Event::decode_exact(&[0x02, 0x00, 0x07]).is_ok());
//! let err = Event::decode_exact(&[0x09, 0x00, 0x07]).unwrap_err();
//! assert_eq!(err.field, Some("version"));
//! assert!(matches!(err.kind, ErrorKind::Convert { .. }));
//! // Encode is untouched by the guard — forging stays possible:
//! assert_eq!(Event { version: 9, id: 7 }.to_bytes().unwrap(), [0x09, 0x00, 0x07]);
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
//! ## Ready-made codecs — [`bnb::codecs`](crate::codecs)
//!
//! Before rolling your own, check the shipped library: LEB128 varints, NUL-terminated
//! C strings, and length-prefixed strings, all referenced by path. (The `read_pascal`
//! above is exactly what [`codecs::prefixed`](crate::codecs::prefixed) ships — checked
//! and UTF-8-validated.)
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Packet {
//!     #[br(parse_with = bnb::codecs::leb128::parse)]
//!     #[bw(write_with = bnb::codecs::leb128::write)]
//!     length: u64,
//!     #[br(parse_with = bnb::codecs::prefixed::parse_string::<_, u8>)]
//!     #[bw(write_with = bnb::codecs::prefixed::write_string::<_, u8>)]
//!     name: String,
//! }
//!
//! let p = Packet { length: 300, name: "Hi".into() };
//! assert_eq!(p.to_bytes().unwrap(), [0xAC, 0x02, 0x02, b'H', b'i']);
//! assert_eq!(Packet::decode_exact(&p.to_bytes().unwrap()).unwrap(), p);
//! ```
//!
//! ## Shared encode state — [`Sink::scratch`](crate::Sink::scratch)
//!
//! A `write_with` sees only its own field and the `Sink`. When a codec needs mutable state
//! shared across *every* field of a message — a **back-reference / compression dictionary**
//! (e.g. DNS name compression: a repeated name becomes a pointer to the first occurrence) —
//! attach it to the sink with [`BitWriter::with_scratch`](crate::BitWriter::with_scratch) and
//! reach it via `w.scratch()` + [`downcast_mut`](core::any::Any::downcast_mut). The sink is the
//! one `&mut` threaded through all fields, so the value is visible to them all; it is dropped on
//! `into_bytes` and never written. (Scope the borrow — copy what you need out before calling
//! `w.write`.)
//!
//! ```
//! use bnb::{bin, BitEncode, BitWriter, Sink, BitError, Source};
//!
//! // First write of a byte records its offset; a repeat emits a 0xFF marker + that offset.
//! fn backref<K: Sink>(v: &u8, w: &mut K) -> Result<(), BitError> {
//!     let at = (w.bit_pos() / 8) as u8;
//!     let prior = w.scratch()
//!         .and_then(|s| s.downcast_mut::<std::collections::HashMap<u8, u8>>())
//!         .and_then(|d| match d.get(v).copied() { Some(o) => Some(o), None => { d.insert(*v, at); None } });
//!     match prior { Some(o) => { w.write(0xFFu8)?; w.write(o) } None => w.write(*v) }
//! }
//! fn read<S: Source>(r: &mut S) -> Result<u8, BitError> { r.read() }
//!
//! #[bin(big)]
//! struct M { #[bw(write_with = backref)] #[br(parse_with = read)] a: u8,
//!            #[bw(write_with = backref)] #[br(parse_with = read)] b: u8 }
//!
//! let mut w = BitWriter::new().with_scratch(Box::new(std::collections::HashMap::<u8, u8>::new()));
//! M { a: 7, b: 7 }.bit_encode(&mut w).unwrap();
//! assert_eq!(w.into_bytes(), [7, 0xFF, 0x00]); // second 7 → back-reference to offset 0
//! ```
//!
//! ## Per-type codecs — `#[bin(codec = …)]` newtypes
//!
//! When the same codec applies to *many* fields, hoist it onto a **newtype**: a
//! single-field tuple struct whose wire form is owned by the fn pair. The type then
//! carries its codec everywhere — fields need no attributes at all (just
//! `#[brw(variable)]`, below).
//!
//! ```
//! use bnb::bin;
//!
//! /// A LEB128-encoded u64 — annotate once, use as a plain field forever.
//! #[bin(codec = bnb::codecs::leb128)]           // the module's `parse`/`write` pair…
//! #[derive(Debug, Clone, Copy, PartialEq)]
//! pub struct Varint(pub u64);
//! // …or any fns: #[bin(codec(parse = <f>, write = <f>))] — turbofish welcome,
//! // e.g. parse = bnb::codecs::prefixed::parse_string::<_, u16>.
//!
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Frame {
//!     kind: u8,
//!     #[brw(variable)]      // a variable-length type in an otherwise-fixed parent
//!     length: Varint,       // ← the codec travels with the type
//!     crc: u16,
//! }
//!
//! let f = Frame { kind: 1, length: Varint(300), crc: 0xBEEF };
//! assert_eq!(f.to_bytes().unwrap(), [0x01, 0xAC, 0x02, 0xBE, 0xEF]);
//! assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
//! assert_eq!(u64::from(Varint(300)), 300); // `From` both ways comes generated
//! ```
//!
//! The newtype gets `BitDecode`/`BitEncode`, the slice entry points
//! (`decode_exact`/`decode_all`/`to_bytes`, at its own declared `big`/`little`/
//! `bit_order` — a *field* of this type decodes through the parent's cursor), and
//! `From` conversions both ways. It emits **no `FixedBitLen`** — a codec's wire form
//! is assumed variable; a genuinely fixed-width codec adds the one-line manual impl
//! (see [`mapping`](super::mapping)). `read_only`/`write_only` narrow the direction
//! (the paren form may then omit the unneeded fn). For a one-off field, plain
//! `parse_with`/`write_with` stays the right tool.
//!
//! ## `#[brw(variable)]` — a variable-length field in a fixed parent
//!
//! A struct with no `Vec` or codec directives normally derives `FixedBitLen` by
//! summing its fields — which fails to compile against a variable-length custom type
//! (the error names the missing `FixedBitLen`). `#[brw(variable)]` declares the truth:
//! this field's width isn't fixed, so the parent never claims to be. It's harmlessly
//! redundant on a field that is already variable (a `Vec`, a directive-bearing field).
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
//! // Decode is verbatim — it captures the actual reserved bits off the wire...
//! let actual = R::decode_exact(&[0x01, 0x55, 0x55, 0x02]).unwrap();
//! assert_eq!((actual.pad, actual.ones), (0x55, 0x55));
//! assert_eq!(actual.to_bytes().unwrap(), [0x01, 0x55, 0x55, 0x02]); // re-emitted as-is
//! // ...while `to_canonical_bytes` writes the reserved fields' spec values instead.
//! assert_eq!(actual.to_canonical_bytes().unwrap(), [0x01, 0x00, 0xFF, 0x02]);
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
//! a seekable source, so `decode` on a forward-only stream is a compile error; the
//! slice paths (`decode_exact`/`decode_all`/`peek`) always qualify.
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
//!
//! # `seek` — read at an absolute offset (pointer-following)
//!
//! `#[br(seek = <bits>)]` jumps the cursor to an **absolute** bit offset before reading
//! the field — the building block for offset tables and pointer chains. Bit/byte amounts
//! come from the [`prelude`](crate::prelude) (`ptr.bytes()`, `n.bits()`). It is read-side
//! (the writer is append-only); pair it with `restore_position` to read at the offset and
//! return so later fields continue in order. Like `restore_position` it seeks, so
//! `decode` on a forward-only stream is a compile error; the slice paths qualify.
//!
//! ```
//! use bnb::{bin, prelude::*};
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Ptr {
//!     ptr: u8,                                     // byte offset of `target`
//!     #[br(seek = ptr.bytes(), restore_position)]
//!     target: u8,                                  // read at `ptr`, then rewind
//!     next: u8,                                    // continues right after `ptr`
//! }
//!
//! // `peek` doesn't require full consumption (seek/restore leave the tail untouched).
//! let p = Ptr::peek(&[0x03, 0x11, 0x22, 0xAB]).unwrap();
//! assert_eq!((p.ptr, p.target, p.next), (3, 0xAB, 0x11));
//! ```
//!
//! On encode the seek is a no-op (the writer appends), so a *relocated* layout won't
//! round-trip through the default encoder — emit such formats with `write_with` /
//! `write_only`, where you control placement.
//!
//! # `dbg` — trace a field as it decodes
//!
//! `#[br(dbg)]` emits a [`tracing`](https://docs.rs/tracing) event as the field is read,
//! carrying its start bit offset and decoded value (the field type must be `Debug`). It
//! is a read-side diagnostic — no extra bits are consumed and encode is unaffected. The
//! event is at `TRACE` level under the `bnb::dbg` target, so you can surface just these
//! with `RUST_LOG=bnb::dbg=trace` (the application installs the subscriber; libraries
//! only emit).
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq)]
//! struct Framed { tag: u8, #[br(dbg)] len: u16 }
//!
//! // Decoding is identical with or without `dbg`; it just also traces `len`.
//! let f = Framed::decode_exact(&[0x01, 0x00, 0x2A]).unwrap();
//! assert_eq!(f, Framed { tag: 1, len: 42 });
//! ```
//!
//! # `try_str` — render a byte buffer as a string in `Debug`
//!
//! `#[try_str]` is a **rendering hint**, not a codec directive: a byte-buffer field (`Vec<u8>`
//! / `[u8; N]`) prints in `Debug` as a quoted, escaped **string** when its bytes are valid
//! UTF-8, and falls back to **hex bytes** otherwise — all-or-nothing, never lossy (no `�`). It
//! changes nothing on the wire: the field still stores raw bytes (sized by `count`, etc.), so
//! the parser stays permissive — a non-UTF-8 value decodes fine, it just renders as bytes.
//! `Debug` is what `tracing`'s `?` and `{:#?}` use, so this is what tidies up log output.
//!
//! ```
//! use bnb::bin;
//! #[bin(big)]
//! #[derive(Debug, PartialEq, Eq)]
//! struct Record {
//!     id: u8,
//!     #[br(temp)] #[bw(calc = self.name.len() as u8)] len: u8,
//!     #[br(count = len)] #[try_str] name: Vec<u8>,
//! }
//!
//! let text = Record { id: 1, name: b"hi".to_vec() };
//! assert!(format!("{text:?}").contains(r#"name: "hi""#));   // valid UTF-8 -> "hi"
//!
//! let bin = Record { id: 2, name: vec![0xC0, 0xDE] };
//! assert!(format!("{bin:?}").contains("name: [c0, de]")); // not UTF-8 -> hex bytes
//! ```
