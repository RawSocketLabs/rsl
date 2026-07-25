# Hypothetical Repository Onboarding

This example demonstrates the workflow; it is not a policy template to copy
without an interview.

## 1. Inspect

Assume `telemetry-wire` is a public Rust library that decodes and encodes a
versioned telemetry protocol. The read-only inspector reports:

- one library package and no workspace members;
- edition 2024, stable toolchain, and no declared MSRV;
- public `encode` and `decode` modules;
- protocol vector tests and Criterion benchmarks;
- a `fuzz/` directory that is not run in CI;
- `#![forbid(unsafe_code)]`;
- a dependency with default features enabled;
- formatting, Clippy, tests, and docs in CI;
- no repository instructions, `cargo-deny` policy, or SemVer check.

These are observations, not decisions. The inspector proposes
`public-library + protocol + public-api + performance-sensitive` for
confirmation.

## 2. Interview and decision summary

After grouped questions, the owner confirms:

- The crate is public and maintained for backward-compatible releases.
- Stable Rust is required; the MSRV will be declared and checked.
- The wire specification and published errata are authoritative. Fields use
  specification bit numbering in MSB-first transmission order; byte order is
  stated per multi-octet field.
- Unknown non-reserved values are preserved for inspection and forward
  compatibility. Reserved encodings are rejected unless the specification
  explicitly assigns preservation behavior.
- Strict construction, finite parser budgets, distinguishable incomplete and
  malformed results, and independent known-answer vectors are required.
- Round-trip properties complement but do not replace known-answer and
  interoperability evidence.
- Unsafe remains forbidden. Zero-copy work must remain safe and measured.
- Criterion tracks representative decode workloads. A readable reference
  decoder remains until an optimized form is independently easy to verify.
- Unit, integration, doctest, boundary, property, vector, and fuzz targets are
  required. Fuzzing has a short pull-request smoke tier and a longer scheduled
  tier.
- `cargo fmt`, Clippy, tests, docs, feature checks, MSRV, dependency policy, and
  SemVer checks define successful release-candidate changes.

The unresolved item is the sustained fuzzing cadence before the first public
release.

## 3. Approved profile and skills

```text
base: public-library
capabilities: protocol + public-api + performance-sensitive

enabled:
  rust-core
  rust-implement
  rust-review
  rust-api-design
  rust-testing
  rust-protocol
  rust-performance
  rust-dependencies-security
```

The proposal is presented in full and approved before any file is written.

## 4. Preview and render

The approved answers follow
`skills/rust-repository-onboarding/assets/approved-answers.example.json`.
Preview:

```text
skills/rust-repository-onboarding/scripts/render_adoption.py \
  --answers /tmp/telemetry-wire-approved.json \
  --target /work/telemetry-wire \
  --standards 0.1.0 \
  --source rust-skills-v0.1.0
```

After reviewing the path and content preview, render:

```text
skills/rust-repository-onboarding/scripts/render_adoption.py \
  --answers /tmp/telemetry-wire-approved.json \
  --target /work/telemetry-wire \
  --standards 0.1.0 \
  --source rust-skills-v0.1.0 \
  --write
```

The renderer refuses to overwrite an existing path. Because this hypothetical
repository had no instructions, it creates:

```text
AGENTS.md
.rust-skills/
├── adoption.toml
├── repository-profile.md
├── enabled-skills.md
├── validation.toml
├── validation.md
├── decisions/
│   ├── compatibility.md
│   ├── protocol.md
│   └── testing.md
└── unresolved.md
scripts/
└── validate-rust.py
```

In a mature repository, `agents_mode = "preserve"` would leave existing
instructions untouched and generate an adoption index or proposal instead.
The generated `adoption.toml` records the run date, exact source pin, adapter,
generated-manifest SHA-256, and a bundle hash for every enabled skill.

## 5. Install and validate

Install only the confirmed skills:

```text
cargo xtask install \
  --agent common \
  --scope repo \
  --target /work/telemetry-wire \
  --skills rust-core,rust-implement,rust-review,rust-api-design,rust-testing,rust-protocol,rust-performance,rust-dependencies-security
```

Then run:

```text
python3 scripts/validate-rust.py
python3 scripts/validate-rust.py --json
```

Each configured check reports `passed`, `failed`, `skipped`, `unavailable`, or
`inapplicable`. An unavailable required tool fails the workflow; a disabled
optional Miri check is reported as skipped with its recorded reason. The agent
presents the generated diff and results for final confirmation and leaves the
fuzz cadence in `.rust-skills/unresolved.md`.
