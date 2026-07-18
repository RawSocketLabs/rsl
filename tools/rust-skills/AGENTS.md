# RSL Rust engineering skills

This directory is the canonical source for RSL's reusable Rust engineering
skills. It is an independently versioned, std-only Cargo workspace and is not a
member of the repository's root Cargo workspace.

## Ownership and boundaries

- Author runtime skill content only under `skills/`.
- Treat `generated/` as deterministic output. Change canonical sources and run
  `cargo xtask generate`; never hand-edit generated adapters.
- Keep repository facts and exceptions in the adopting repository's
  `AGENTS.md`, not in reusable skills.
- Keep the component relocatable and resolve resources relative to this
  directory or an individual skill package.
- Do not install generated adapters into RSL discovery paths as part of a
  source or documentation change. Activation is a separate, explicit adoption
  decision.
- Discuss any third-party dependency before adding it. The initial tooling is
  intentionally standard-library-only.
- Preserve the RSL workspace MSRV of Rust 1.85 unless a reviewed repository
  decision changes it.

## Verify

Run from `tools/rust-skills`:

```sh
cargo xtask validate
cargo xtask generate --check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Keep eval task prompts isolated from graders and expected answers. Independent
baseline-versus-skill runs are a publication gate, not an implementation
artifact to reconstruct from the checked-in fixtures.
