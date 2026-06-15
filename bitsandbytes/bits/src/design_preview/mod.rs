//! # `bnb` — target-design preview (NOT YET IMPLEMENTED)
//!
//! <div class="warning">
//!
//! **This is a design artifact, not shipped API.** Every type, trait, macro, and
//! attribute described here is the *intended end state* of the codec currently
//! spiked in `bits` (to be renamed `bnb`). Nothing in this module is implemented;
//! the code blocks are illustrative (` ```rust,ignore `). Its purpose is to let
//! you review the API an end user would see — laid out like binrw's attribute
//! reference — and react **before** we build it. Rationale: `DESIGN.md` §10–§11;
//! build sequence: `ROADMAP.md`.
//!
//! Everything *outside* this module documents `bits` as it ships **today** (a
//! bit/byte library with `binrw` integration). This module is the target `bnb`
//! that the `ROADMAP.md` migrates toward — they coexist on purpose.
//!
//! </div>
//!
//! ## What `bnb` is
//!
//! A from-scratch, **bit-aware** binary codec for protocol messages: declarative,
//! bidirectional read/write derived from one struct, like binrw — but it reads and
//! writes fields at **arbitrary bit offsets**, not just byte boundaries, and it
//! does not require `Seek`. It owns both the bit layer and the byte layer (hence
//! *bits-and-bytes*), so there is no external codec dependency.
//!
//! It deliberately **inherits binrw's attribute grammar** (`#[br]`/`#[bw]`/`#[brw]`
//! and its sub-keys). binrw's design is excellent; reusing its vocabulary means a
//! binrw user is immediately at home. See [`compared_to_binrw`](crate::design_preview::compared_to_binrw)
//! and `ACKNOWLEDGMENTS.md`.
//!
//! ## The mental model (read this first)
//!
//! 1. **One macro: [`#[bin]`](crate::design_preview::bin_macro).** You annotate a struct; you get
//!    a bidirectional codec + a builder + field accessors.
//! 2. **One vocabulary, dispatched per field.** `#[br]` = read-only directive,
//!    `#[bw]` = write-only, `#[brw]` = both. A byte-aligned field and a sub-byte
//!    field use the *same* attributes; the macro routes each to the right backend.
//! 3. **Bit-aware by default.** A `u108`, a `#[bitfield]`, a `#[derive(BitEnum)]`
//!    just work as fields, even straddling byte boundaries. See
//!    [`bit_fields`](crate::design_preview::bit_fields).
//! 4. **Seek-free by default.** Parsing reads from an owned byte buffer with a bit
//!    cursor — seeking is cursor arithmetic, no `Seek` trait, no `NoSeek`. See
//!    [`io_model`](crate::design_preview::io_model).
//! 5. **Compliant by default, deliberately violatable.** Every guided default has
//!    an escape hatch for fuzzing/red-teaming/interop. See
//!    [`escape_hatches`](crate::design_preview::escape_hatches).
//!
//! ## The pages
//!
//! - [`quick_start`](crate::design_preview::quick_start) — three complete examples (byte header,
//!   bit frame, mixed).
//! - [`bin_macro`](crate::design_preview::bin_macro) — the `#[bin]` attribute: struct-level
//!   options + what it generates.
//! - [`attributes`](crate::design_preview::attributes) — **the reference**: every `br`/`bw`/`brw`
//!   attribute.
//! - [`bit_fields`](crate::design_preview::bit_fields) — the bit-level surface unique to `bnb`.
//! - [`io_model`](crate::design_preview::io_model) — readers/writers, seeking, `forward_only`,
//!   streaming.
//! - [`escape_hatches`](crate::design_preview::escape_hatches) — the dual-use escape hatches.
//! - [`compared_to_binrw`](crate::design_preview::compared_to_binrw) — what maps 1:1, what's new,
//!   migration & credit.
//!
//! ## Naming conventions (reviewed & approved)
//!
//! The names this design commits to. Every prior open question has been resolved.
//!
//! | Concept | Proposed name | binrw analog | Notes |
//! |---|---|---|---|
//! | The one macro | `#[bin]` | `#[binrw]` | `read_only`/`write_only` flags for directional codecs |
//! | Read trait | `Decode` | `BinRead` | split read/write (some types are read-only) |
//! | Write trait | `Encode` | `BinWrite` | |
//! | Byte source | `Source` (trait) | `Read` | impl'd for `&[u8]` (consume) + any `Read`; `decode` takes `&mut impl Source` |
//! | Seekable source | `SeekSource` (trait) | `Read + Seek` | adds seek; impl'd by `BitReader`, `BufSource`, `File`; seek-using messages bound on it |
//! | Socket+seek adapter | `BufSource<R>` | `NoSeek` (but real) | retains read bytes → seek within a **bounded** window; reads more on demand |
//! | Byte sink | `Sink` (trait) | `Write` | impl'd for any `Write` + `BitWriter`; `encode` takes `&mut impl Sink` |
//! | In-memory cursor | `BitReader` / `BitWriter` | `Cursor` | slice-backed, free seek; the explicit positioned form |
//! | Result/error | `bnb::Result` / `bnb::Error` | `BinResult` / `binrw::Error` | position-aware; `Incomplete { needed: Option<usize> }` for streaming |
//! | Bit substrate | `Bits`, `u1..u127` | — | already shipped |
//!
//! Each design decision is recorded inline on its page as a **“✓ decided”** note.

pub mod attributes;
pub mod bin_macro;
pub mod bit_fields;
pub mod compared_to_binrw;
pub mod escape_hatches;
pub mod io_model;
pub mod quick_start;
