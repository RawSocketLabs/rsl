# rfus

> Inherits the workspace-root `../AGENTS.md`.

Parses human-written RF quantities (frequencies, bandwidths, sample rates, scan targets) into
exact integer domain types. Integer semantics are the contract: a value either resolves to a
whole Hz or S/s, or it is rejected with a specific `ParseUnitError`. Never round, and never
substitute a fabricated value in an error.

## Rules

- `#![forbid(unsafe_code)]`; `std` only, with an optional `serde` feature.
- Validity rules live in constructors: `FrequencyRange::new` (`lower < upper`) and
  `FrequencyRanges::new` (non-empty). Parsing and serde must go through them. Do not reintroduce
  public fields or enum payloads that bypass them.
- No RF range bounds. Device limits belong to consumers. The only bound is fitting the
  backing integer (`u32` for sample rates and `Hertz::as_u32`).
- Serde wire format is a compatibility surface. Scalars serialize as integers and deserialize from
  integers (including TOML's `i64`) or strings. Binary formats read the raw integer.
  `ScanTarget` is untagged and needs a self-describing format.
- `ParseUnitError` is `#[non_exhaustive]`. Add a variant rather than overload `Parse` with
  semantic failures.
- The free `parse_*` helpers and `FrequencyRange::canonical` are intentional convenience entry
  points. Keep them consistent with `FromStr` and `Display`.

## Consumers

- `rsl` re-exports this crate as `rsl::rf` behind its optional `rf` feature. Check
  `cargo check -p rsl --features rf` after any public API change.
