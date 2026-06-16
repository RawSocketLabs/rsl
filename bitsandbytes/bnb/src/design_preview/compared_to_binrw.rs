//! # Compared to binrw — mapping, what's new, migration & credit
//!
//! `bnb` is binrw-shaped on purpose. If you know binrw, this page is the whole
//! delta. (Target design; ` ```rust,ignore `.)
//!
//! ## Credit
//!
//! The attribute grammar, the read/write split, and the declarative bidirectional
//! model are **binrw's design** ([jam1garner/binrw](https://github.com/jam1garner/binrw),
//! MIT). `bnb` reuses that vocabulary and is a from-scratch reimplementation (no
//! binrw source copied); see `ACKNOWLEDGMENTS.md`. The obligation we hold
//! ourselves to: where we reuse a spelling, it **means what binrw means**.
//!
//! ## Maps 1:1 (same name, same semantics)
//!
//! `#[br]`/`#[bw]`/`#[brw]`, `magic`, `calc`, `temp`, `ignore`, `map`/`try_map`,
//! `count`, `if`, `pad_*`/`align_*`, `restore_position`, `assert`/`pre_assert`,
//! `parse_with`/`write_with`. The builder's `default` mirrors `derive_builder`.
//!
//! ## Renamed
//!
//! | binrw | bnb | Why |
//! |---|---|---|
//! | `#[binrw]` | `#[bin]` | the crate's verb; `bnb::bin` |
//! | `BinRead` / `BinWrite` | `Decode` / `Encode` | match the macro |
//! | `BinResult` / `binrw::Error` | `bnb::Result` / `bnb::Error` | position-aware |
//! | `#[binread]` / `#[binwrite]` | `#[bin(read_only)]` / `#[bin(write_only)]` | flag on one macro, not separate macros |
//! | `import(...)` / `args { … }` | `ctx(...)` / `ctx { … }` | clearer; declares/passes context |
//!
//! ## New in `bnb` (no binrw analog)
//!
//! - **Bit-level fields** — `u1..u127`, `#[bitfield]`, `#[derive(BitEnum)]`,
//!   `#[bitflags]` as fields at arbitrary bit offsets, no `map` glue
//!   ([`super::bit_fields`]).
//! - **`bit_order = msb \| lsb`** — independent of byte order.
//! - **Seek-free default + `forward_only`** — no `NoSeek`; attribute-driven IO
//!   bound ([`super::io_model`]).
//! - **The right-tool guard** — `#[bin]` rejects an all-byte-aligned bit message
//!   and points at the plain path.
//! - **Dual-use as a first-class contract** ([`super::escape_hatches`]).
//!
//! ## Deferred / out of scope (for now)
//!
//! `bnb` targets **bounded protocol messages**, not arbitrary file/container
//! formats. Consciously deferred (see `DESIGN.md` §11 DD2): parsing inputs larger
//! than memory and deep random-access formats (ELF/ZIP/fonts) over a non-buffered
//! stream. The streaming backend ([`super::io_model`]) opens the door; until then
//! the in-memory cursor is the model.
//!
//! ## Interop & migration
//!
//! During the rebuild (`ROADMAP.md`), a `binrw-compat` feature keeps the bridge:
//! a `#[bin]` message can embed a binrw type (and vice versa) via
//! `parse_with`/`write_with`. So adoption is incremental — you are never forced to
//! migrate a whole crate at once, and binrw stays usable for the deferred cases.
//!
//! ```rust,ignore
//! // A binrw type embedded in a bnb message during migration:
//! #[bin(big)]
//! struct Mixed {
//!     header: BitHeader,                       // native bnb (bit fields)
//!     #[br(parse_with = binrw_bridge::read)]   // a legacy #[binrw] body
//!     body: LegacyBody,
//! }
//! ```
//!
//! ## Summary
//!
//! Keep binrw's ergonomics; add bit-awareness and a seek-free model; own the whole
//! stack so the dependency can be dropped. If a spelling here surprises a binrw
//! user, that's a bug in this design — tell us.
