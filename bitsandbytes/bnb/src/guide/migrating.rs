//! Coming from `binrw`, `modular-bitfield`, or `num_enum` — how the mental models map.
//!
//! `bnb` folds the jobs of several crates into one surface (see the crate's
//! `ACKNOWLEDGMENTS.md`), and its
//! attribute vocabulary deliberately echoes theirs — so if you already think in one of
//! them, most of your instincts carry over. This page maps each crate's spellings to
//! `bnb`'s and, more importantly, calls out where they genuinely differ.
//!
//! `bnb` examples here are runnable doctests; the other crates aren't dependencies, so
//! their snippets are illustrative (shown `ignore`d or in prose).
//!
//! # From `binrw`
//!
//! [`binrw`](https://docs.rs/binrw) is the closest relative: `#[bin]`'s
//! `#[br]`/`#[bw]`/`#[brw]` attribute split is modeled directly on it, and where a
//! spelling is reused it means what binrw means. A binrw user is immediately at home.
//!
//! ## Directive mapping
//!
//! Most directives are spelled identically:
//!
//! | binrw | `bnb` | notes |
//! |-------|-------|-------|
//! | `#[br(..)]` / `#[bw(..)]` / `#[brw(..)]` | same | read / write / both |
//! | `#[br(big)]` / `#[br(little)]` | `#[bin(big)]` / `#[bin(little)]` | byte order — see below |
//! | `magic = b"..."` / `magic = 0u8` | struct: `magic = <Bits expr>`; enum dispatch: `magic = b"..."` | same idea; sub-byte magics allowed |
//! | `count = <expr>` | `count = <expr>` | length-driven `Vec` |
//! | `calc = <expr>` | `calc = <expr>` | computed field (pair with `temp`) |
//! | `temp` | `temp` | read into a local, don't store |
//! | `map = <f>` / `try_map = <f>` | `map = <f>` / `try_map = <f>` | transform the wire value |
//! | `if(<cond>)` | `if(<cond>)` | conditional `Option` |
//! | `parse_with = <f>` / `write_with = <f>` | `parse_with = <f>` / `write_with = <f>` | custom field codec |
//! | `ignore` / `default` | `ignore` (as `#[brw(ignore)]`) | neither read nor written |
//! | `assert(<cond>)` | `assert(<cond>)` (as `#[br(assert(..))]`) | decode-time guard; `bnb`'s is read-only |
//! | `pad_before` / `pad_after` | `pad_before` / `pad_after` | in **bits**, not bytes |
//! | `align_before` / `align_after` | `align_before` / `align_after` | to the next byte boundary |
//! | `restore_position` | `restore_position` | peek without consuming |
//! | `seek_before` | `seek` | jump to an absolute offset |
//! | `dbg` | `dbg` (`std`-only; a `tracing` event) | trace a field as it decodes |
//! | `import` / `args` | `ctx(..)` + field `ctx { .. }` | parse context; `bnb`'s is decode-only |
//!
//! So a binrw struct like this:
//!
//! ```ignore
//! # // binrw — illustrative, not compiled here.
//! #[binrw]
//! #[brw(big, magic = b"BM")]
//! struct Header {
//!     #[br(temp)]
//!     #[bw(calc = items.len() as u16)]
//!     count: u16,
//!     #[br(count = count)]
//!     items: Vec<u16>,
//! }
//! ```
//!
//! becomes, in `bnb` — nearly the same, and the length-prefixed idiom collapses to one
//! directive. (A struct-level `magic` takes a [`Bits`](crate::Bits) value — here the
//! `u16` `0x424D` = `"BM"`; a *byte-string* magic like binrw's `b"BM"` is the
//! [enum-dispatch](super::dispatch) spelling, verified by peeking the wire.)
//!
//! ```
//! use bnb::bin;
//!
//! #[bin(big, magic = 0x424Du16)]   // "BM"
//! #[derive(Debug, PartialEq)]
//! struct Header {
//!     #[brw(count_prefix = u16)]   // the temp+calc+count triad, in one line
//!     items: Vec<u16>,
//! }
//!
//! let h = Header { items: vec![0xAABB, 0xCCDD] };
//! assert_eq!(h.to_bytes().unwrap(), [b'B', b'M', 0x00, 0x02, 0xAA, 0xBB, 0xCC, 0xDD]);
//! assert_eq!(Header::decode_exact(&h.to_bytes().unwrap()).unwrap(), h);
//! ```
//!
//! ## The real differences
//!
//! - **Bit offsets, not just byte offsets.** binrw is byte-oriented — it reads and
//!   writes through `io::Read`/`io::Write` + `Seek`, so a field starts on a byte
//!   boundary. `bnb` reads and writes at arbitrary **bit** offsets: a `u4`, a `u12`, a
//!   3-bit flag run, or a whole `#[bitfield]` sits mid-byte with no manual shifting. This
//!   is the capability that separates the two, and it is why `pad_before`/`seek` amounts
//!   are counted in **bits** (via `n.bits()` / `n.bytes()` from the
//!   [`prelude`](crate::prelude)), not bytes.
//! - **Byte order is spelled `bytes` (and there's a `bits` knob too).** binrw sets
//!   endianness with `big`/`little`. `bnb` uses the same `big`/`little` keywords —
//!   `#[bin(big)]` is sugar — but the full spelling is `#[bin(bytes = big|little)]`, and
//!   there is a *second*, independent knob binrw has no equivalent for: `bits = msb|lsb`,
//!   the bit order within a byte. The two compose by the natural-layout rule (see
//!   [`bin_codec`](super::bin_codec)).
//! - **`no_std` + `alloc`.** Both are `no_std`; `bnb` additionally ships a push/pull
//!   framing buffer ([`BitBuf`](crate::BitBuf), with a bounded/alloc-once mode) for
//!   embedded transports, plus an [I/O ladder](super::io) that scales from `&[u8]` up to
//!   `std` streams, `bytes`, and `tokio` framing.
//! - **A dual-use encode model.** [`to_bytes`](crate::BitEncode) is *verbatim* —
//!   byte-identical to what you decoded, retained reserved bits and all — while
//!   `to_canonical_bytes` normalizes (`reserved` → spec, `calc` recomputed). You pick per
//!   call. binrw always recomputes on write; `bnb` lets you observe and re-emit a peer's
//!   exact bytes, which matters for a security tool. See [`dual_use`](super::dual_use).
//! - **More than a codec.** binrw is read/write only. `bnb` also gives you
//!   [`#[bitfield]`](super::bitfields), [`#[bitflags]`](super::flags), and
//!   [`BitEnum`](super::enums) as first-class field types (below), plus
//!   [`WireLen`](crate::WireLen)/`auto_len` for auto-deriving, overridable length fields.
//! - **Whole-struct wire mapping.** binrw's `map`/`try_map` transform a *field*; `bnb`
//!   adds the same at the struct level (`#[bin(wire = W)]` / `#[bin(map = .., bw_map = ..)]`),
//!   a whole logical type serialized via a separate wire type. See [`mapping`](super::mapping).
//!
//! A few binrw directives have no direct `bnb` equivalent (or a different home):
//! `import_raw`/`args_raw`, `offset` (binrw's `FilePtr`), `try_calc`, `pre_assert`, and
//! `pad_size_to`. Reach for `parse_with`/`write_with` or `seek` + `restore_position` for
//! those shapes.
//!
//! # From `modular-bitfield`
//!
//! [`modular-bitfield`](https://docs.rs/modular-bitfield) (and the similar
//! [`bitfield-struct`](https://docs.rs/bitfield-struct)) packs sub-byte fields into a
//! byte array. `bnb`'s [`#[bitfield]`](super::bitfields) does the same job, backed by a
//! single unsigned integer, and — crucially — the result is a [`Bits`](crate::Bits)
//! value that drops straight into a `#[bin]` message.
//!
//! ## Field types and widths
//!
//! modular-bitfield uses opaque specifier types `B1`, `B2`, … `B128`, with an optional
//! `#[bits = N]` compile-time check. `bnb` uses the **real arbitrary-width integers**
//! `u1`..`u127` as the field types, so a field's width is usually *inferred* from its
//! type — no separate specifier:
//!
//! | modular-bitfield | `bnb` |
//! |------------------|-------|
//! | `a: B5` | `a: u5` (width inferred) |
//! | `a: B1` | `a: bool` or `a: u1` |
//! | `#[bits = 5] a: SomeType` | `#[bits(5)] a: SomeType` (note: parens, not `=`) |
//! | `#[skip] __: B3` | a `#[bits(A..=B)]` gap, or just don't name those bits |
//! | struct-level `bits = 32` guard | the backing integer, e.g. `#[bitfield(u32, ..)]` |
//!
//! ```ignore
//! # // modular-bitfield — illustrative.
//! #[bitfield]
//! struct VersionIhl { version: B4, ihl: B4 }
//! ```
//!
//! ```
//! use bnb::{bitfield, u4};
//!
//! #[bitfield(u8, bits = msb, bytes = big)]
//! #[derive(Clone, Copy)]
//! struct VersionIhl { version: u4, ihl: u4 }
//!
//! let v = VersionIhl::new().with_version(u4::new(4)).with_ihl(u4::new(5));
//! assert_eq!(v.to_raw(), 0x45);
//! ```
//!
//! ## Accessors
//!
//! The accessor shapes line up closely:
//!
//! | modular-bitfield | `bnb` |
//! |------------------|-------|
//! | `fieldname()` (getter, panics on invalid) | `fieldname()` (returns the typed value) |
//! | `set_fieldname(v)` | `set_fieldname(v)` |
//! | `with_fieldname(v)` | `with_fieldname(v)` (chainable) |
//! | `set_fieldname_checked` / `with_..._checked` | field types are range-checked at construction |
//! | `new()` | `new()` (all-zero) |
//! | `from_bytes([u8; N])` / `into_bytes()` | `from_bytes(..)` / `to_bytes()` (declared `bytes` order) |
//! | — | `to_raw()` / `from_raw()` (the backing integer directly) |
//! | — | `to_be_bytes` / `to_le_bytes` (endianness override) |
//!
//! ## What `bnb` adds
//!
//! - **Explicit bit *and* byte order.** `#[bitfield(u16, bits = msb, bytes = big)]` — the
//!   first-declared field lands in the high (`msb`) or low (`lsb`) bits, and `to_bytes`
//!   serializes in the declared byte order. modular-bitfield fixes an LSB-first layout.
//! - **It nests into `#[bin]`.** A `bnb` bitfield is a `Bits` value, so it drops into a
//!   whole-message [`#[bin]`](super::bin_codec) struct as a single field, and other
//!   bitfields / `BitEnum`s nest *inside* it — all width-checked at compile time.
//! - **A custom `Debug`.** `#[derive(Debug)]` on a `bnb` bitfield decomposes the *logical*
//!   fields (`version: 4, ihl: 5`) rather than printing the opaque backing integer.
//!
//! ```
//! use bnb::{bin, bitfield, u4};
//!
//! #[bitfield(u8, bits = msb, bytes = big)]
//! #[derive(Clone, Copy, Debug)]
//! struct VersionIhl { version: u4, ihl: u4 }
//!
//! // The bitfield is just another field of a whole-message struct.
//! #[bin(big)]
//! #[derive(Debug)]
//! struct Ipv4Start { vi: VersionIhl, tos: u8, total_len: u16 }
//!
//! let h = Ipv4Start {
//!     vi: VersionIhl::new().with_version(u4::new(4)).with_ihl(u4::new(5)),
//!     tos: 0,
//!     total_len: 40,
//! };
//! assert_eq!(h.to_bytes().unwrap(), [0x45, 0x00, 0x00, 0x28]);
//! ```
//!
//! # From `num_enum`
//!
//! [`num_enum`](https://docs.rs/num_enum) maps a `#[repr(uN)]` enum to and from its
//! primitive discriminant. `bnb`'s [`#[derive(BitEnum)]`](super::enums) does the same and
//! goes sub-byte: a `bnb` enum can be 3 or 12 bits wide, and it nests into a
//! `#[bitfield]` or a `#[bin]` message.
//!
//! ## Derive and attribute mapping
//!
//! | num_enum | `bnb` |
//! |----------|-------|
//! | `#[derive(TryFromPrimitive, IntoPrimitive)]` | `#[derive(BitEnum)]` |
//! | `#[repr(u8)]` (required) | `#[bit_enum(u8)]` + `#[repr(u8)]` |
//! | `#[derive(FromPrimitive)]` + `#[num_enum(default)]` | `#[catch_all]` (a total, lossless `From`) |
//! | `#[num_enum(catch_all)] Other(u8)` | `#[catch_all] Other(u8)` (tuple variant holds the width) |
//! | closed set (no default) → `TryFromPrimitive` | `closed` → checked `TryFrom` |
//! | `TryFromPrimitiveError` | [`UnknownDiscriminant`](crate::UnknownDiscriminant) |
//! | `#[num_enum(alternatives = [..])]` | (fold into `#[catch_all]`, or match on the raw value) |
//!
//! num_enum's `catch_all` variant must be a tuple variant holding the exact repr type;
//! `bnb`'s is the same, holding the width type (which *is* the repr type for a primitive
//! width). A num_enum enum:
//!
//! ```ignore
//! # // num_enum — illustrative.
//! #[derive(TryFromPrimitive, IntoPrimitive)]
//! #[repr(u8)]
//! enum EtherType { Ipv4 = 0x00, Arp = 0x06 }
//! ```
//!
//! becomes, with a catch-all so decoding is total and lossless (the dual-use default):
//!
//! ```
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u8)]
//! #[repr(u8)]
//! enum EtherType { Ipv4 = 0x00, Arp = 0x06, #[catch_all] Other(u8) }
//!
//! // The num_enum-parity primitive conversions come for free at primitive widths:
//! assert_eq!(u8::from(EtherType::Arp), 0x06);              // IntoPrimitive
//! assert_eq!(EtherType::from(0x06u8), EtherType::Arp);     // total (has catch-all)
//! assert_eq!(EtherType::from(0x99u8), EtherType::Other(0x99));
//! ```
//!
//! For a set you assert is closed, mark it `closed` and you get the checked `TryFrom`,
//! erroring with [`UnknownDiscriminant`](crate::UnknownDiscriminant) — num_enum's
//! `TryFromPrimitive`/`TryFromPrimitiveError` role:
//!
//! ```
//! #[derive(bnb::BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
//! #[bit_enum(u8, closed)]
//! #[repr(u8)]
//! enum Direction { Request = 1, Reply = 2 }
//!
//! assert_eq!(Direction::try_from(2u8), Ok(Direction::Reply));
//! assert!(Direction::try_from(7u8).is_err());   // UnknownDiscriminant
//! ```
//!
//! ## What `bnb` adds
//!
//! - **Bit-native reprs.** num_enum requires a primitive `#[repr]`. `bnb` enums can be
//!   *sub-byte* (`#[bit_enum(u3)]`) or byte-aligned-but-non-primitive (`#[bit_enum(u24)]`).
//!   At those widths the enum gets the [`Bits`](crate::Bits) impls but not the primitive
//!   `From`/`TryFrom` (there's no `u3` primitive to convert with) — it's meaningful only
//!   nested where its bits are placed in context.
//! - **It nests.** A `BitEnum` drops into a [`#[bitfield]`](super::bitfields) or a
//!   [`#[bin]`](super::bin_codec) message as one field, contributing exactly its declared
//!   bits.
//! - **The `num_enum` parity is automatic at primitive widths.** A `u8`/`u16`/`u32`/`u64`/
//!   `u128`-width enum emits the same `From`/`TryFrom<primitive>` you'd reach num_enum for
//!   — no separate derive, no hand-written round-trip test. See [`enums`](super::enums).
//!
//! # Where to go next
//!
//! - [`quick_start`](super::quick_start) — the five-minute tour if you skipped it.
//! - [`bin_codec`](super::bin_codec) / [`directives`](super::directives) — the `#[bin]`
//!   surface in full (the binrw analogue).
//! - [`bitfields`](super::bitfields) / [`enums`](super::enums) / [`flags`](super::flags) —
//!   the field types (the modular-bitfield / num_enum analogues).
//! - [`composition`](super::composition) — how all of the above nest and size each other.
