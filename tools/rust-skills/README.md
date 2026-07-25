# RSL Rust Engineering Skills

This directory is the canonical source for a portable Rust engineering system
with a separate Raw Socket Labs organization layer. It contains a modular skill
catalog, composable repository profiles, trust-tiered references, migration
contracts, generated agent adapters, and comparative eval fixtures. It is
independently versioned within the RSL monorepo and may also be exported as a
standalone bundle without joining a consumer's Cargo workspace.

The 14 canonical runtime packages are:

```text
rust-core                    rust-implement
rust-review                  rust-repository-onboarding
rust-api-design              rust-testing
rust-protocol                rust-dsp
rust-performance             rust-async-concurrency
rust-unsafe-ffi              rust-dependencies-security
rust-embedded                rust-skill-maintenance
```

`rsl-rust-core` and `rsl-rust-review` remain thin compatibility routers for
existing consumers. A default install selects only the canonical packages, so
new repositories do not activate duplicate legacy identities.

Canonical runtime content lives under `skills/`. Generated install views under
`generated/` are derived artifacts and must match
`cargo xtask generate --check`. Repository facts and exceptions belong in
repository-local instructions, not reusable skills.

## Development commands

Run these commands from `tools/rust-skills`:

```text
cargo xtask validate
cargo xtask generate
cargo xtask generate --check
cargo xtask install --agent common --scope repo --target /path/to/repository
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

From the RSL repository root, use an explicit manifest path when scripting the
component, for example:

```text
cargo run --manifest-path tools/rust-skills/Cargo.toml --package xtask -- validate
```

Read [adoption](docs/adoption.md) before applying the standards to a repository
and [authoring conventions](docs/authoring-conventions.md) before changing a
skill or rule. The current structure is defined by the
[modular architecture](docs/architecture.md), its
[repository audit](docs/audit-2026-07-24.md),
[gap analysis](docs/gap-analysis.md), and the completed
[v0.1 migration](docs/migrations/0.1-to-modular.md). Use the
[task-to-skill map](docs/task-to-skill-map.md) to select capability owners and
the [hypothetical onboarding example](docs/onboarding-example.md) to understand
the approval-gated adoption workflow.
Design provenance remains in the
[preference record](docs/preference-record.md),
[research report](docs/research-report.md), and
[borrowed-option and frozen-sequence synthesis](docs/research/borrowed-option-and-frozen-sequences.md),
and the
[historical v0.1 architecture proposal](docs/architecture-proposal.md).

## License

Licensed under either Apache License, Version 2.0 or the MIT license, at your
option.
