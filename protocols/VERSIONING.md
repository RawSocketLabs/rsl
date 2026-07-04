# Versioning

This workspace versions **each crate independently** (its own `version` in its `Cargo.toml`) and
also carries a **workspace version** (`[workspace.package].version`) as a release-train milestone
marker. Everything else — `edition`, `license`, `repository`, `rust-version`, dependency
versions, lints — is shared from the workspace.

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

- Per-crate: `[package] version` in each crate's `Cargo.toml`.
- Workspace milestone: `[workspace.package] version` — **not** inherited by any crate; bump it
  when cutting a coordinated milestone (its minor tracks broad capability milestones, its patch
  smaller rollups).
- Current versions + stage: the crate status table in [`AGENTS.md`](AGENTS.md).

## Publishing

Pre-1.0 and co-evolving with `bnb`, this workspace **does not publish to crates.io yet**
(`release-plz.toml: publish = false`) — release-plz manages versions, changelogs, and git tags
only. Publishing flips on later (add `CARGO_REGISTRY_TOKEN`, set `publish = true`).
