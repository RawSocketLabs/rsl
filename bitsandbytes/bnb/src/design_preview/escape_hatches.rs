//! # Escape hatches (dual-use)
//!
//! `bnb` is **compliant by default, deliberately violatable**. The guided path
//! emits and accepts RFC-correct traffic; a caller who needs to send or parse
//! non-conformant traffic (fuzzing, red-teaming, interop, security research) must
//! be able to. Every default has a documented hatch. (Target design;
//! ` ```rust,ignore `.)
//!
//! This mirrors the workspace rule (root `AGENTS.md`): *never enforce a policy
//! requirement inside a parser or raw constructor.*
//!
//! ## The hatches, by layer
//!
//! | Hatch | What it bypasses | Where |
//! |---|---|---|
//! | `from_raw` / `from_bits` | range/layout validation | constructor |
//! | `#[catch_all]` / `Custom(..)` | "unknown value ⇒ error" | enum decode |
//! | permissive `Decode` | all assertions/validators | parser |
//! | `skip_validation()` | the construction soundness check (only when `#[bin(validate=…)]`) | builder |
//! | `allow_byte_aligned` | the right-tool guard | `#[bin]` |
//! | `parse_with` / `write_with` | the generated codec entirely | field |
//! | `peek(&[u8])` | input consumption (decode without advancing) | entry point |
//! | struct literal `Frame { .. }` | the builder *and* validation entirely (pub fields) | construction |
//!
//! ## Parsers never reject representable input
//!
//! `Decode` decodes anything the layout can represent. Unknown enum
//! discriminants become `Custom(..)` / `#[catch_all]` variants rather than errors;
//! field access masks rather than validates. So a fuzzer's malformed-but-encodable
//! frame round-trips instead of being refused.
//!
//! ```rust,ignore
//! // RSV is 0 by the builder default, but the field stays writable. The plain
//! // struct literal (pub fields) is the rawest construction — no builder, no
//! // validation:
//! let evil = Frame { rsv: u3::new(0b101), /* …other fields… */ };
//! ```
//!
//! ## Compliant defaults live on the builder, not the parser
//!
//! `version = 5`, `rsv = 0`, etc. are builder defaults — correct out of the box,
//! and the same fields stay `pub`/overridable. The parser imposes none of them.
//!
//! ## Validation is opt-in, construction-only, and lives on the Builder
//!
//! `#[bin(validate = path)]` (`path: fn(&FrameBuilder) -> Result<(), impl Display>`)
//! is **optional** — most types declare none. When present, it checks **structural
//! soundness** — "do these fields form a well-formed struct?" (a length matches its
//! payload, a checksum is consistent) — and is run by `Builder::build()`; failure is
//! `BuilderError::Invalid`. It is deliberately **not** a method on the concrete type:
//! "valid in a protocol *conversation*" (a legal response, an in-window sequence) is
//! the session/state-machine layer's job, never the codec's.
//!
//! The escape hatch exists only alongside a declared validator: `skip_validation()`
//! on the builder bypasses it. `decode` is unaffected — it never validates (the
//! dual-use rule), so checking *received* data is the application's concern.
//!
//! ### Constructing: three tiers
//!
//! | Construct | Required fields | Validation |
//! |---|---|---|
//! | `Frame { .. }` (struct literal) | by hand | none — the rawest path |
//! | `Frame::builder()…build()?` | enforced | runs the validator, if any |
//! | `…skip_validation().build()?` | enforced | bypassed (only if a validator exists) |
//!
//! ## The universal hatch: `parse_with` / `write_with`
//!
//! If `bnb` does not model something, drop to a raw function for that field and do
//! anything. Nothing about the codec is a closed door.
//!
//! > **✓ decided:** no `build_unchecked()`/`build_raw()`/`raw::build()`. Validation
//! > is opt-in and **Builder-bound** (construction soundness, not protocol-context);
//! > the bypass is `skip_validation()`, generated only when `#[bin(validate=…)]` is
//! > present. The rawest construction is the plain `pub`-field struct literal. (`raw`
//! > stays the name for value-level `from_raw`/`from_bits` — a different layer.)
//!
//! Next: [`super::compared_to_binrw`].
