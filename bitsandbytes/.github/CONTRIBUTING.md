# Contributing to bitsandbytes

Thanks for your interest in `bitsandbytes` (imported as `bnb`). Bug reports, docs fixes,
tests, and well-scoped ideas are all genuinely welcome — this guide explains how the
project is run and how to make a change land smoothly.

## How this project is run (please read first)

`bnb` is built and maintained by **RawSocketLabs** and used in our products, so its
direction is **product-first**: changes are judged primarily by how well they serve those
use cases and the [ROADMAP](../bnb/ROADMAP.md). We're happy to hear good ideas and will
engage with them, but the maintainers make the final call on what's accepted — and not
every proposal will land. That's not a knock on the work; it's how we keep the API surface
intentional on the road to 1.0.

In practice:

- **Bug reports, documentation, and tests** — always welcome.
- **Features, new `#[bin]` directives, or API changes** — open an issue first (see below)
  so we can confirm the fit before you invest time.
- We may **decline or defer** changes that widen scope beyond what our products need.
- **Security issues**: do **not** open a public issue or PR — follow [SECURITY.md](SECURITY.md).

## Before you write code

- **Trivial changes** (typos, doc tweaks, small bug fixes): just open a PR.
- **Anything non-trivial** (features, refactors, API or directive changes): **open an issue
  or discussion first.** Agreeing on the approach up front avoids wasted work on something
  we'd decline.

## Development workflow

1. **Fork** the repo and create a branch off `main`.
2. Make your change — keep each PR to **one concern**.
3. Run the local checks below until they're green.
4. Open a **PR against `main`**. CI must pass and a maintainer must approve; we
   **squash-merge** and delete the branch.

### Commits

Commits must follow **[Conventional Commits](https://www.conventionalcommits.org/)** —
commitlint enforces this in CI, and `release-plz` derives each crate's SemVer bump and
`CHANGELOG.md` from them. Allowed types are in
[`commitlint.config.mjs`](../.config/commitlint.config.mjs): `feat`, `fix`, `docs`, `test`,
`refactor`, `chore`, `ci`, `perf`, `style`, `build`, `bench`, `revert`. Only `feat`
(minor) and `fix` (patch) move a version; a breaking change uses `!` or a
`BREAKING CHANGE:` footer (on `0.x` that's a minor bump).

### Local checks (these mirror CI)

```bash
cargo fmt --all                              # rustfmt
cargo clippy --workspace --all-targets       # clippy::all is denied
cargo test --workspace                       # the suite (default features)
cargo test -p bitsandbytes --features bytes  # + the bytes-crate I/O adapters
cargo +1.85.0 check --workspace              # MSRV floor (1.85)
# no_std proof — build the smoke crate for a bare-metal target (std off):
cargo build --manifest-path bnb/nostd-check/Cargo.toml --target thumbv7em-none-eabi
```

### If your change touches the public API

Two CI gates guard the surface; regenerate their baselines **deliberately** when an API
change is intended (and call it out in the PR):

- **`public-api`** — snapshot diff. Regenerate with the pinned toolchain:
  ```bash
  cargo +nightly-2026-06-17 public-api -p bitsandbytes --all-features > bnb/public-api.txt
  ```
- **`semver-checks`** — *informational* (non-blocking): it flags a SemVer-breaking change
  vs the last release as a heads-up. Mark a breaking change with `!` / a `BREAKING CHANGE:`
  footer; `release-plz` turns it into the right version bump when it cuts the release PR
  (it owns versioning — don't hand-bump versions in a feature PR).

## Code expectations

- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"` — contributions must stay
  within it (the codec reaches `1.0` with a zero-`unsafe` guarantee).
- **Document public items.** `#![deny(missing_docs)]` is on; new public API needs docs.
- **Add tests** for new behavior, following the `tests/` layout (one concern per file);
  compile-fail cases go in `tests/ui/*` via `trybuild` (regenerate with `TRYBUILD=overwrite`).
- **Mind the MSRV (1.85):** let-chains are unstable below 1.88 — don't use them; the
  `cargo +1.85.0 check` above catches it.
- **Pick the right tool** (`#[bitfield]` vs `#[bin]` vs the bare derives) — the
  right-tool guard rejects all-byte-aligned bare-derive structs. The design rationale and
  internal invariants live in [`bnb/DESIGN.md`](../bnb/DESIGN.md) and the contributor/agent
  guide [`bnb/AGENTS.md`](../bnb/AGENTS.md).

## Review & merge

PRs are reviewed and merged by the **RawSocketLabs maintainers** (currently the project's
two owners). A merge requires **green CI** and a **maintainer approval**; we squash-merge.
Expect review to weigh the change against the product use cases and the ROADMAP.

## Licensing of contributions

Unless you explicitly state otherwise, any contribution you intentionally submit for
inclusion in this project shall be **dual-licensed under [MIT](../LICENSE-MIT) OR
[Apache-2.0](../LICENSE-APACHE)** — the same terms as the project — with no additional terms
or conditions. **No CLA or sign-off is required.**
