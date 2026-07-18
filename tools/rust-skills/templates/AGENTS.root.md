# Repository Instructions

## Scope and map

- Repository purpose: `[required]`
- Important crates/applications and ownership boundaries: `[required]`
- Generated or externally owned paths that agents must not edit: `[if any]`

## Adopted Rust standards

- Standards pin: `[exact release or commit from rsl-rust-standards.toml]`
- Default profile: `[public-library | internal-library | performance-application | pragmatic-application | prototype]`
- Component profile or domain overrides: `[only real differences]`

Apply current user instructions first, then the closest nested `AGENTS.md`, this
file, declared domain guidance, and general Rust guidance. Surface material
conflicts instead of silently choosing a lower-precedence rule.

## Canonical commands

- Format/check: `[required]`
- Lint: `[required]`
- Fast tests: `[required if distinct]`
- Default pull-request tests: `[required]`
- Extended tests: `[if applicable]`
- Adversarial tests, fuzzing, or sanitizers: `[if applicable]`
- Performance benchmarks and profiling: `[if applicable]`
- Documentation and examples: `[if applicable]`

State which commands actually ran and which evidence remains unavailable.

## Toolchain, platforms, and dependencies

- Exact MSRV and current-development toolchain: `[required]`
- First-class targets and native-test expectations: `[required]`
- Dependency policy and whether `rsl-deps` is adopted: `[required]`
- Feature combinations required in CI: `[if applicable]`

Discuss material dependency changes before editing manifests. A change is
material when it expands features or the resolved graph, raises MSRV, changes
unsafe exposure, or changes behavior.

## Architecture and risk boundaries

- Public API and compatibility commitments: `[required for reusable libraries]`
- Trust boundaries and protocol authorities: `[if applicable]`
- Hot paths, performance budgets, and allocation constraints: `[if applicable]`
- Queue overload, backpressure, cancellation, and shutdown policy: `[if applicable]`
- Unsafe and FFI locations plus verification commands: `[if applicable]`

## Documentation, examples, and fixtures

- Required public/module documentation and vocabulary: `[if applicable]`
- Example inventory and canonical invocation: `[if applicable]`
- Fixture provenance, storage, and regeneration: `[if applicable]`
- ADR, changelog, and generated-file rules: `[if applicable]`

## Local exceptions

List each exception with its exact scope, rationale, owner, and removal condition
when temporary. Omit this section when no exceptions exist.
