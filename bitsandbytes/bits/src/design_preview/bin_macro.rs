//! # The `#[bin]` macro
//!
//! One attribute folds the protocol-message triad — **codec + builder + bit
//! packing** — into a struct. It is the unified successor to the spike's
//! `#[bitwire]`/`#[wire]`. (Target design; ` ```rust,ignore `.)
//!
//! ```rust,ignore
//! #[bin(big, validate = Frame::check)]
//! #[derive(Debug, Clone, PartialEq)]
//! struct Frame { /* fields */ }
//! ```
//!
//! ## Directional forms
//!
//! The codec can be read-only, write-only, or both — chosen by a **flag on the one
//! macro** (consistent with `forward_only`), not a separate macro:
//!
//! | Form | Generates | Use |
//! |---|---|---|
//! | `#[bin(...)]` | `Decode` + `Encode` + builder | the default |
//! | `#[bin(read_only)]` | `Decode` only (no builder; struct literal for tests) | parse-only messages |
//! | `#[bin(write_only)]` | `Encode` + builder | emit-only messages |
//!
//! ## Struct-level options
//!
//! | Option | Meaning | Default |
//! |---|---|---|
//! | `big` / `little` | byte order on the wire | `big` |
//! | `bit_order = msb \| lsb` | first declared bit lands high or low | `msb` |
//! | `magic = <lit>` | a leading constant for the whole message | none |
//! | `forward_only` | pin a `Read`-only bound; a seek directive becomes a compile error (see [`super::io_model`]) | off |
//! | `read_only` / `write_only` | generate only `Decode` / only `Encode` (+ builder); default is both | both |
//! | `ctx(name: Ty, …)` | declare context this type needs from its parent (binrw `import`) | none |
//! | `validate = <path>` | `fn(&Builder) -> Result<(), impl Display>` — **construction** soundness, run by `build()` (not protocol-context; not a method on the type). Adds `skip_validation()` to the builder. The parser stays permissive. | none |
//! | `no_builder` | skip builder generation (codec only) | off |
//! | `stream` | target a streaming `Read`/`Write` backend instead of the in-memory cursor | off (in-memory) |
//!
//! ## What it generates
//!
//! - **`Decode` / `Encode`** + entry points (full detail in [`super::io_model`]):
//!   - `Frame::decode(&mut impl Source)` — easy button; consumes from a `&[u8]`
//!     view or any `Read`; tail-tolerant; `Incomplete` ⇒ need more bytes.
//!   - `Frame::decode_exact(&[u8])` — strict: errors unless fully consumed.
//!   - `Frame::peek(&[u8])` — decode the front **without** consuming.
//!   - `Frame::decode_from(&mut BitReader)` — explicit cursor for seek/overlap/
//!     many-messages.
//!   - `frame.encode(&mut impl Sink)` (any `Write`) / `frame.to_bytes()` /
//!     `frame.encode_into(&mut BitWriter)`.
//!
//!   Equivalent in spirit to binrw's `BinRead`/`BinWrite`, but you never wrap a
//!   slice in a reader for the common case (binrw makes you use `Cursor`), and a
//!   forward-only message needs only `Read`, never `Read + Seek`.
//! - **A builder** — `Frame::builder()…build() -> Result<Frame, bnb::Error>`;
//!   required-by-default fields, `#[bin(default)]` opts out. Compliant defaults
//!   live here, not in the parser.
//! - **Accessors** for any packed bit-group, so collapsed fields stay first-class.
//! - **`validate`** wiring *only if* `#[bin(validate = …)]` is given: `build()` runs
//!   the `fn(&Builder)` check; the builder gains `skip_validation()`. No method on
//!   the concrete type — construction soundness, not protocol-conversation validity.
//!
//! ## Field model
//!
//! A field is, in order of precedence:
//! 1. a **directive-driven** field (`#[br]`/`#[bw]`/`#[brw]` — see [`super::attributes`]);
//! 2. a **bit field** — any [`Bits`](crate::Bits) type (`u1..u127`, `#[bitfield]`,
//!    `#[derive(BitEnum)]`, `#[bitflags]`), read at the current bit offset (see
//!    [`super::bit_fields`]);
//! 3. a **plain byte field** — `u8`/`u16`/`Vec<T>`/nested `#[bin]` — read at the
//!    (byte-aligned) cursor.
//!
//! The macro picks the backend per field; you never say which.
//!
//! > **✓ decided:** directional codecs are **flags on `#[bin]`** —
//! > `#[bin(read_only)]` / `#[bin(write_only)]` — not separate macros. Consistent
//! > with `forward_only`, one macro, unambiguous. (binrw's separate
//! > `#[binread]`/`#[binwrite]` shape rejected.)
//!
//! > **✓ decided:** `bit_order` is **per-struct only** (`#[bin(bit_order = msb|lsb)]`).
//! > Mixed order within a message is expressed by a nested `#[bitfield]`/group with
//! > its own `bits = msb|lsb` — the natural, *named* unit for a run of ordered bits.
//! > No per-field override (avoids scope ambiguity; redundant with the group level).
//!
//! Next: [`super::attributes`].
