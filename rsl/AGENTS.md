# rsl — agent & contributor guide

> `CLAUDE.md` is a symlink to this file so every AGENTS.md-aware tool reads the same rules.

**What this is.** The RSL **owned-library** facade: a feature-gated crate that re-exports the
public libraries RSL authors (codec, protocols, rawsock, rf, usdr) under semantic paths. Owns
**no logic** — a namespacing + version-unification layer. The blessed *third-party* crates live
in the separate `rsl-deps` crate; the private owned crates in `rsl-private`. See
[`README.md`](README.md) for the namespace map and rationale.

## Invariants

- **Owned crates only.** Only RSL-authored libraries belong here. Blessed third-party crates go
  in `rsl-deps`; never add a `crates.io` dependency to `rsl`.
- **Public boundary.** Only owned crates with **public** `RawSocketLabs` repos. Never add a
  private crate (libsdr, rust-dsdcc) or a customer name/URL — those go in `rsl-private`.
- **Every dependency is `optional`.** `default = []`; the crate compiles to nothing until a
  feature is enabled.
- **Pinned revs, one codec.** Owned crates are `git` deps pinned to an exact `rev`. `bnb` and
  the `protocols` crates must resolve to the **same** `bitsandbytes` rev or the `#[bin]`
  proc-macro breaks on a duplicate — the `protocols` repo rev-pins bnb for this reason. When
  bumping, keep rsl's `bnb` rev equal to the one `protocols`'s pinned rev uses.
- **`publish = false`.** Git deps can't appear in a crates.io release; `rsl` is consumed as a
  git dependency.

## Adding an owned crate

1. Add it to `[dependencies]` as `optional = true`, a `git` dep pinned to a `rev`. (Private
   owned crate? It belongs in `rsl-private`. Third-party crate? `rsl-deps`.)
2. Add a feature enabling `dep:<name>` (name it after the capability).
3. Re-export it in `src/lib.rs` under a semantic path (`pub use … as …;`), gated on the feature.
4. Update the tables in `README.md` and, if common, `prelude`.
5. Extend `tests/namespaces.rs`.

## Verify

```sh
cargo build  --features full            # codec, proto, rawsock, rf
cargo test   --features codec,proto,rf
cargo clippy --features codec,proto,rawsock,rf
cargo deny   --all-features check
```
The FFI feature `usdr` needs a C++ toolchain (off by default).
