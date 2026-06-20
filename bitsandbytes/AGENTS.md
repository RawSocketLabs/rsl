# AGENTS.md

Project-level guidance for AI agents. Intentionally short — the **detailed engineering
guide is [`bnb/AGENTS.md`](bnb/AGENTS.md)** (codec/macro internals, test layout, gotchas);
read it before changing `bnb`/`bnb-macros`.

## What this is

A Cargo workspace for **`bitsandbytes`** (imported as `bnb`), an owned, bit-aware binary
codec, plus its proc-macro crate. Two members:

- `bnb/` — runtime library (published as `bitsandbytes`, lib name `bnb`).
- `bnb-macros/` — proc-macros (published as `bitsandbytes-macros`; re-exported by `bnb`).

Overview in [`README.md`](README.md); rationale in [`bnb/DESIGN.md`](bnb/DESIGN.md); status
and the road to 1.0 in [`bnb/ROADMAP.md`](bnb/ROADMAP.md).

## Working here

- **One concern per change**, on a branch off `main` → PR → green CI → squash-merge.
- **Conventional Commits** are enforced (commitlint); `release-plz` derives versions from
  them. `feat`/`fix` bump; most other types don't.
- **CI gates** (all must pass): fmt, clippy (`clippy::all` denied), test (+ `--features
  bytes`), `no_std` (bare-metal), cargo-deny, MSRV 1.85, fuzz, public-api, semver-checks.
- **Zero `unsafe`** — `unsafe_code = "forbid"` workspace-wide; don't introduce any.
- **Public items need docs** (`#![deny(missing_docs)]`); a public-API change means
  regenerating `bnb/public-api.txt` (see `bnb/AGENTS.md` and the `public-api` CI job).

## Layout

The root holds only what must live there (`Cargo.toml`, `rust-toolchain.toml`, `LICENSE-*`,
`README.md`, `rustfmt.toml`, `release-plz.toml`, this file). Relocatable tool configs are in
`.config/` (`deny.toml`, `commitlint.config.mjs`); community-health docs in `.github/`
(`CONTRIBUTING.md`, `SECURITY.md`); release process in `docs/`. Run cargo-deny locally via
`cargo deny-check` (alias in `.cargo/config.toml`).

## Contributors & security

[`.github/CONTRIBUTING.md`](.github/CONTRIBUTING.md) · [`.github/SECURITY.md`](.github/SECURITY.md)
