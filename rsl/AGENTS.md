# rsl — agent & contributor guide

> `CLAUDE.md` is a symlink to this file so every AGENTS.md-aware tool reads the same rules.

**What this is.** The RSL stack facade: a single, feature-gated crate that re-exports the
blessed internal and external crates RSL application layers build on. It owns **no logic** —
it is a namespacing + version-unification layer. See [`README.md`](README.md) for the full
namespace map, feature list, and design rationale.

## Invariants

- **Every dependency is `optional`.** `default = []`; the crate compiles to nothing until a
  feature is enabled. Never add a non-optional dependency.
- **`Cargo.toml` is the single source of truth for versions.** All blessed pins live in one
  `[dependencies]` table. Bump in one place; the whole ecosystem moves together.
- **Owned vs. external split is load-bearing.** Owned RSL crates get semantic paths
  (`rsl::codec`, `rsl::proto`, …). Blessed externals live under `rsl::ext::*`. When an owned
  crate supersedes an external, graduate it out of `ext` into a semantic path and delete the
  external pin — see the "graduation path" section in the README.
- **Public boundary.** This is the PUBLIC facade: only blessed externals + owned crates with
  **public** `RawSocketLabs` repos. Never add a private crate (libsdr, rust-dsdcc) or a
  customer name/URL here — those go in the private overlay crate. Owned crates are `git` deps
  (one source per crate); `bnb` shares the protocol crates' git source so the codec unifies
  with no `[patch]`.
- **`publish = false`.** Git deps can't appear in a crates.io release; `rsl` is consumed as a
  git dependency.

## Adding a crate to the stack

1. Add it to `[dependencies]` as `optional = true` (git for owned/public, versioned for
   external). Private owned crate? It belongs in the private overlay, not here.
2. Add a feature that enables `dep:<name>` (name the feature after the capability, not the
   crate, when it's a group).
3. Re-export it in `src/lib.rs`: owned → a semantic `pub use … as …;`; external → under
   `pub mod ext`. Gate every re-export on its feature.
4. Update the tables in `README.md` and, if it's a common item, `prelude`.
5. Extend `tests/namespaces.rs` if the pure-Rust CI slice covers it.

## Verify

```sh
cargo build  --features codec,proto,rawsock,std-ext,bytes,netutil,hash,parse,num,audio,fft
cargo test   --features codec,proto,std-ext
cargo clippy --features codec,proto,std-ext
```
The FFI feature `usdr` needs a C++ toolchain (off by default).
