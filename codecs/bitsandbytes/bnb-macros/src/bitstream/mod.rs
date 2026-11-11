//! Expansion of `#[derive(BitDecode)]` / `#[derive(BitEncode)]` — the bit-stream
//! message codec.
//!
//! Each generates an impl that reads/writes the struct's named fields **in
//! declaration order** from a `bnb::BitReader`/`BitWriter` bit cursor. A field
//! is read with `__bnb_r.read()` / written with `__bnb_w.write(self.field)`, which works for
//! any `bnb::Bits` type (`u1`..`u127`, `#[bitfield]`, `#[derive(BitEnum)]`), so
//! the bit-stream codec composes with the rest of the crate's macros. Nested
//! `#[nested]` messages, `[u8; N]` payloads, `magic`, `#[br(count = …)]` `Vec`s,
//! `ctx` parameterization, `#[br(temp)]`/`#[bw(calc = …)]`, `#[br(if(…))]`
//! conditional `Option`s, `#[br(map/try_map = …)]`/`#[bw(map = …)]` transforms, and
//! `#[reserved]`/`#[reserved_with(…)]` bits, positioning (`pad_*`/`align_*`/
//! `restore_position`), and the `parse_with`/`write_with` escape hatches are all
//! supported (`temp`/`calc`/`reserved` via `#[bin]`, which generates the codec
//! directly).
//!
//! ## Right-tool guard
//!
//! The bare `#[derive(BitDecode/BitEncode)]` is the low-level bit codec; if a
//! struct's fields are **all byte-aligned** (every width a multiple of 8) the cursor
//! never leaves byte boundaries, so `#[bin]` (the unified codec) is the better tool.
//! The derives emit a const-eval guard that rejects such a struct, steering the
//! author to `#[bin]`. The escape hatch is `#[bit_stream(allow_byte_aligned)]`.
//!
//! ## Layout
//!
//! - `field` — the field-directive model: parsing, validation, `count_prefix`.
//! - `ctx` — the `ctx` parameter grammar.
//! - `generate` — the shared generators (`gen_decode`/`gen_encode`) and their helpers.
//! - `derive` — the bare-derive front-end.
//! - `bin` — the `#[bin]` front-ends (structs, wire-mapped structs, codec newtypes, enums).

mod bin;
mod ctx;
mod derive;
mod field;
mod generate;

pub(crate) use bin::{expand_bin, field_normalizes_aliases};
pub(crate) use derive::{expand_decode, expand_encode};
