# rsl-deps — agent & contributor guide

> `CLAUDE.md` is a symlink to this file. See [`README.md`](README.md) for the overview.

**What this is.** The RSL blessed **external**-dependency stack: one feature-gated crate that
pins and re-exports the third-party crates RSL libraries and apps build on. Owns no logic — a
version-unification + re-export layer. The *owned* half of the stack lives in the separate `rsl`
facade.

## Invariants

- **External crates only.** Never add an owned RSL crate here — those go in `rsl`. This crate must
  have **no git dependencies** (only registry version pins) so it stays publishable.
- **Every dependency is `optional`.** `default = []`; nothing compiles until a feature is enabled.
- **`Cargo.toml` is the single source of truth for versions.** Bump in one place; the ecosystem
  moves together. Keep versions aligned with what the owned crates and apps already resolve to.
- **Re-export under canonical names** at the crate root (`rsl_deps::tokio`), gated on the feature.
- **`publish = false` for now**, but keep it publishable (no git deps) for the eventual registry move.

## Adding a blessed crate

1. Add it to `[dependencies]` as `optional = true` with a version pin.
2. Add a capability feature enabling `dep:<name>` (group related crates under one feature).
3. Re-export it at the crate root under its canonical name, gated on the feature; add common
   items to `prelude` if warranted.
4. Update the tables in `README.md`.
5. Add its license to `deny.toml`'s allow-list if cargo-deny flags it (deliberately, never to
   silence a real problem).

## Verify

```sh
cargo build  --features full
cargo test   --features full
cargo clippy --features full
cargo deny   --all-features check
```
