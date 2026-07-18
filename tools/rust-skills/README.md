# RSL Rust Engineering Skills

This directory is the canonical source for Raw Socket Labs Rust engineering
judgment, packaged as two portable Agent Skills, repository-adoption templates,
generated agent adapters, and comparative eval fixtures. It is independently
versioned within the RSL monorepo and may also be exported as a standalone
bundle without joining a consumer's Cargo workspace.

Initial skills:

- `rsl-rust-core` guides implementation and material Rust changes.
- `rsl-rust-review` reviews Rust changes for actionable correctness and
  regression risks.

Canonical content lives under `skills/`. Generated install views under
`generated/` are derived artifacts and must match `cargo xtask generate --check`.
Repository facts and exceptions belong in human-authored `AGENTS.md` files, not
in reusable skills.

## Development commands

Run these commands from `tools/rust-skills`:

```text
cargo xtask validate
cargo xtask generate
cargo xtask generate --check
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
skill or rule. The design provenance remains in the
[preference record](docs/preference-record.md),
[research report](docs/research-report.md), and
[architecture proposal](docs/architecture-proposal.md).

## License

Licensed under either Apache License, Version 2.0 or the MIT license, at your
option.
