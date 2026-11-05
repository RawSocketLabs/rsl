# Versioning

Each protocol crate is versioned **independently** (its own `version` in its `Cargo.toml`).
`edition`, `license`, `repository`, `rust-version`, dependency versions and lints are shared
from the root `rsl` workspace, whose guide ([`AGENTS.md`](../AGENTS.md)) is canonical for the
workspace-wide versioning rules. This file covers the protocol-specific parts.

## Commits drive versions

Commits follow **Conventional Commits**: `type(scope): summary`. The **scope is the crate**
(`feat(dns): …` versions `application/dns`); a workspace-wide change uses `workspace` or no
scope. release-plz parses this history to compute each crate's next version and changelog.

| Commit | Public-API effect | 0.x crate bump | ≥1.0 crate bump |
|---|---|---|---|
| `feat!` / `fix!` / `BREAKING CHANGE:` | breaking | **minor** (0.1.x → 0.2.0) | **major** |
| `feat` | additive | **patch** (0.1.0 → 0.1.1) | **minor** |
| `fix`, `perf` | bug/behaviour fix | **patch** | **patch** |
| `refactor`, `docs`, `test`, `chore`, `ci`, `style`, `build`, `bench` | none | **none** | **none** |

**0.x semantics (Cargo).** For a `0.y.z` crate, `y` is the breaking axis and `z` the compatible
axis — so a breaking change bumps the **minor**, and a new feature bumps the **patch**. A crate
signals "API not yet stable" by staying `0.x`; the first stable release is an explicit `1.0.0`.

**Breaking `!` marker.** Conventional Commits puts the `!` after the scope: `feat(dns)!:`, never
`feat!(dns):` (commitlint rejects the latter).

**Classify by API effect, not verb.** A `refactor` that renames a `pub` item is really a
`feat!`. An internal-dependency bump that surfaces a break is itself breaking for the dependent.

**Stubs are `0.0.0`** (placeholder only). The first real `feat` moves them to `0.1.0`.

## Where versions live

- Per-crate: `[package] version` in each crate's `Cargo.toml` (canonical; release-plz moves it).
- The root workspace's `[workspace.package] version` is a milestone marker shared by every
  product, not a protocol version; no crate inherits it.
- Stage: the crate status table in [`AGENTS.md`](AGENTS.md).

## Publishing

Release processing and publishing are opt-in per package via `[[package]]` overrides in the
root `release-plz.toml`; no protocol crate is currently released (`netlink` has a pending
`publish` opt-in awaiting a crates.io name check). Never flip the workspace defaults.
