# rsl — agent & contributor guide (workspace root)

> `CLAUDE.md` is a symlink to this file. Each crate carries its own `AGENTS.md` with
> crate-specific detail; this root file holds the workspace-wide rules.

**What this is.** The `rsl` monorepo: one Cargo workspace holding the public RawSocket Labs
stack — the codec (`bitsandbytes`), raw-packet I/O (`rawsock`), RF parsing (`rfus`), the
protocol crates (`protocols/*`), the facades (`rsl`, `rsl-deps`), and (excluded, FFI) `usdr`
and `rust-dsdcc`. Crates are **published independently** but developed **in lockstep** — inter-crate
deps are `path` + `version`, so there's one `Cargo.lock` and no git-rev pinning.

## Layout

| Path | Crate(s) | Notes |
|------|----------|-------|
| `bitsandbytes/bnb`, `bitsandbytes/bnb-macros` | `bitsandbytes`, `bitsandbytes-macros` | the codec; `#![forbid(unsafe_code)]` |
| `rawsock/` | `rawsock` | L2/L3/L4 raw I/O; `#![forbid(unsafe_code)]` (safe via `rustix`) |
| `rfus/` | `rfus` | RF/sample-rate parsing |
| `protocols/<layer>/<proto>` | `ethertype`, `ethernet`, `arp`, `tcp`, `udp`, `ip`, `icmp`, `dns` | dual-use protocol codecs on `bnb` |
| `rsl/` | `rsl` | owned-library facade (re-exports the above) |
| `rsl-deps/` | `rsl-deps` | blessed external-dependency stack |
| `usdr/`, `rust-dsdcc/` | `usdr`, `rust-dsdcc` | **excluded** FFI members (need a C++ toolchain) |
| `tools/rust-skills/` | `xtask` (private) | independently versioned Rust engineering skills and adapters; not auto-activated |

## Standards (workspace-wide, in root config)

- **Deps** — inter-crate deps are `path` + `version` in `[workspace.dependencies]`; external
  versions are pinned there (and, for the blessed stack, in `rsl-deps`). `cargo deny` gates
  advisories/licenses; duplicate versions are warned (the facade pulls big trees).
- **Lints** — `[workspace.lints]`; members opt in with `[lints] workspace = true`. The shared
  unsafe policy is `unsafe_op_in_unsafe_fn = deny`; crates that guarantee zero unsafe pin
  `#![forbid(unsafe_code)]` at crate level (bnb, rawsock).
- **Formatting / toolchain / MSRV** — root `rustfmt.toml`, `rust-toolchain.toml`, MSRV 1.85.
- **Versioning** — per-crate SemVer via `release-plz` (scope = crate name, e.g. `feat(dns): …`);
  `publish = false` until the registry decision flips on.
- **Commits** — Conventional Commits (commitlint). **No `Co-Authored-By:` trailer.**

## FFI members (`usdr`, `rust-dsdcc`)

Excluded from the default workspace build/CI (they need a C++ toolchain + system libs). Build
them explicitly: `cargo build --manifest-path usdr/Cargo.toml`. `rust-dsdcc` links the external
DSDcc library — its license governs distributed linked binaries.

## Verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo deny check
```
