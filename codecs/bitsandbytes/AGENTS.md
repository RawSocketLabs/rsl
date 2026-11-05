# AGENTS.md

Project-level guidance for AI agents. Intentionally short — the **detailed engineering
guide is [`bnb/AGENTS.md`](bnb/AGENTS.md)** (codec/macro internals, test layout, gotchas);
read it before changing `bnb`/`bnb-macros`.

## What this is

A directory in the rsl workspace holding the two codec crates of **`bitsandbytes`**
(imported as `bnb`), an owned, bit-aware binary codec, and its proc-macro crate:

- `bnb/` — runtime library (published as `bitsandbytes`, lib name `bnb`).
- `bnb-macros/` — proc-macros (published as `bitsandbytes-macros`; re-exported by `bnb`).

Overview in [`README.md`](README.md); rationale in [`bnb/DESIGN.md`](bnb/DESIGN.md); status
and the road to 1.0 in [`bnb/ROADMAP.md`](bnb/ROADMAP.md).

## Working here

- **One concern per change**, on a branch off `main` → green `ci/circleci: rsl` → fast-forward `main`.
- **Before committing runtime/macro changes**, apply the correctness/performance gate in
  [`docs/RELEASING.md`](docs/RELEASING.md#pre-commit-correctness-and-performance-gate).
  Record candidate evidence and findings in `bnb/DESIGN.md`; obtain final independent review.
- **Conventional Commits** are enforced (commitlint); `release-plz` derives versions from
  them. `feat`/`fix` bump; most other types don't.
- **No `Co-Authored-By:` trailer in commit messages** — do not append the line that makes
  GitHub attribute a commit to a second author (applies to agent- and human-authored commits
  alike). Write a plain subject + body.
- **CI gates** (all must pass): fmt, clippy (`clippy::all` denied), test (+ `--features
  bytes`), `no_std` (bare-metal), cargo-deny, MSRV 1.85, fuzz, public-api, semver-checks.
  These complete gates and affected consumer suites run when bnb is in the impact set.
  Ordinary prose-only changes use the lightweight policy path; release candidates never do.
  Use the root `scripts/ci-local.sh pre-push` selector, not hand-maintained consumer lists.
- **Zero `unsafe`** — `#![forbid(unsafe_code)]` at crate level in both crates; don't introduce any.
- **Public items need docs** (`#![deny(missing_docs)]`); a public-API change means
  regenerating `bnb/public-api.txt` (see `bnb/AGENTS.md` and the `public-api` CI job).

## Layout

Shared configs live at the workspace root (`rustfmt.toml`, `release-plz.toml`, `deny.toml`,
`.config/commitlint.config.mjs`, `.cargo/config.toml`; run cargo-deny locally via
`cargo deny-check`). This directory adds `docs/RELEASING.md` (release process), `fuzz/`
(detached fuzz workspace) and `scripts/` (gdb profiles). `bnb-macros` has no guide of its own:
[`bnb/AGENTS.md`](bnb/AGENTS.md) covers both crates.
