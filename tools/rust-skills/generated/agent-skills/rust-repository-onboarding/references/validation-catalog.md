# Repository Validation Catalog

Select checks from repository risk and policy. Do not enable the entire catalog
mechanically. Store commands as argument arrays and replace illustrative test
filters or benchmark names with repository-confirmed targets.

| Check | Typical command | Default applicability |
|---|---|---|
| Format | `cargo fmt --all -- --check` | Required for ordinary Cargo repositories |
| Compile | `cargo check --workspace --all-targets` | Required unless a more exact target matrix replaces it |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Required at repository-selected lint levels |
| Tests | `cargo test --workspace` | Required; excluded/native/target components need separate entries |
| Documentation tests | `cargo test --workspace --doc` | Required for public libraries when supported |
| Feature matrix | `cargo hack check --workspace --feature-powerset --optional-depth 1` | Required for declared public feature combinations; `required_tools = ["cargo-hack"]` |
| MSRV | `cargo +1.85.0 check --workspace --all-targets` | Required when MSRV is declared; substitute the exact version |
| Miri | `cargo miri test --workspace` | Risk/profile-specific; `required_tools = ["cargo-miri"]` |
| Sanitizers | repository-owned `scripts/test-sanitizers.py` | Unsafe/native/hostile-input profiles on supported nightly targets |
| Loom | repository-selected `cargo test --test loom` | Tractable synchronization models only |
| Fuzz smoke | `cargo fuzz run smoke -- -max_total_time=60` | Hostile parser/unsafe surfaces; use the real target and `required_tools = ["cargo-fuzz"]` |
| Dependency policy | `cargo deny check` | Organization or security profile; `required_tools = ["cargo-deny"]` |
| Advisory audit | `cargo audit` | Repository policy; `required_tools = ["cargo-audit"]` |
| SemVer | `cargo semver-checks check-release` | Public crates with a comparison baseline; `required_tools = ["cargo-semver-checks"]` |
| Coverage | `cargo llvm-cov --workspace --lcov --output-path target/lcov.info` | Advisory evidence, never a correctness substitute; `required_tools = ["cargo-llvm-cov"]` |
| Criterion benchmarks | `cargo bench --workspace` | Measured performance contracts |
| iai-callgrind | repository-selected `cargo bench --bench iai` | Stable instruction/cache investigation on supported hosts |
| Mutation testing | `cargo mutants --workspace --in-place` | Optional suite-quality evidence; `required_tools = ["cargo-mutants"]` |
| Snapshot review | `cargo insta test --workspace --check` | Repositories with deliberately reviewed snapshots; `required_tools = ["cargo-insta"]` |
| Property tests | repository-selected `cargo test --workspace property` | Algebraic/stateful properties; use the actual target/filter |
| Protocol vectors | repository-selected `cargo test --workspace known_answer` | Protocol profiles; independent vectors, not round trips alone |
| Hardware/interop | repository-owned executable script | Only where the environment, credentials, device, or peer implementation exists |

Record prerequisites such as toolchain components, native libraries, targets,
emulators, hardware, credentials, corpus location, comparison baseline, and
acceptable duration. Put Cargo subcommand executable names in `required_tools`
so absence reports `unavailable` rather than an ambiguous command failure.

Use separate fast, pull-request, scheduled, release, hardware, and manual tiers
when duration or environment differs. A required check cannot be disabled. Mark
truly irrelevant checks `inapplicable`; mark deferred optional checks disabled
with a reason. Never call compilation target execution, a round trip
conformance, coverage correctness, or a benchmark general performance proof.
