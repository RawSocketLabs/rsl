# Blessed external versions & drift governance

`rsl-deps` is the single source of truth for the versions of the blessed third-party crates.
Most consume through the facade (`use rsl_deps::tokio`), which unifies their versions automatically.
But **derive-macro crates can't route through a re-export** (serde/thiserror/utoipa hardcode their
own crate name), so consumers keep those as **direct deps** — and this is how we still govern them.

## How it works

- **Source of truth:** the pins in `rsl-deps/Cargo.toml`, emitted to [`blessed-versions.toml`](blessed-versions.toml)
  by `tools/gen-blessed.py` (a CI job fails if the file drifts from the manifest).
- **The check:** `tools/check-drift.py` scans a consumer's `Cargo.toml`s for **direct** deps and flags any
  governed crate on a **different major line** than blessed (e.g. `async-nats = "0.46"` when blessed is `"0.49"`).
  It looks only at direct deps the consumer controls — *not* the whole graph — so normal transitive version
  diversity never causes a false alarm (which is exactly why plain `cargo-deny` bans/`multiple-versions` is the
  wrong tool here: it's graph-wide and flags transitive crates you don't control).

## Wiring it into a consumer

**GitHub Actions** — one step:

```yaml
- uses: RawSocketLabs/rsl/.github/actions/blessed-drift@main
  with:
    strict: "true"   # fail on drift; omit to warn only
```

**GitLab CI** (or any CI / local) — portable, no action needed:

```yaml
blessed-drift:
  image: python:3.12-slim
  script:
    - apt-get update && apt-get install -y curl
    - curl -sSfL https://raw.githubusercontent.com/RawSocketLabs/rsl/main/tools/check-drift.py -o check-drift.py
    - curl -sSfL https://raw.githubusercontent.com/RawSocketLabs/rsl/main/rsl-deps/blessed-versions.toml -o blessed.toml
    - python3 check-drift.py --blessed blessed.toml --repo . --strict
```

## Resolving an alert

Either bump the consumer's direct dep to the blessed major line, or — if the consumer intentionally
leads — **bless the new version in `rsl-deps/Cargo.toml`** (and regenerate). The blessed list moving is
the deliberate, reviewed way to change the whole ecosystem's preferred version.
