# Repository Instructions

## Scope and map

- Repository purpose: `[required]`
- Important crates/applications and ownership boundaries: `[required]`
- Generated or externally owned paths that agents must not edit: `[if any]`

## Adopted Rust standards

- Standards pin: `[exact release or commit from rsl-rust-standards.toml]`
- Base profile: `[public-library | internal-library | application | service | experimental]`
- Capabilities: `[only applicable domain, execution, environment, risk, and structure capabilities]`
- Component overlays: `[only real differences]`

Apply current user instructions first, then the closest directory instruction,
repository decisions and mechanical configuration, confirmed component and
profile defaults, organization preferences, applicable shared skills,
authoritative references, approved guidance, curated examples, advisory or
historical material, and general knowledge. Surface material conflicts instead
of silently choosing a lower-precedence rule.

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
- Protocol validation: strict defaults, named relaxable groups,
  construction/parsing policy split, finite parser budgets with units, scope,
  reset, rationale, and approved overrides, non-disableable safety boundaries,
  post-build validation, policy exclusion from message identity, required
  evidence states and retention, trusted-input boundary or wrapper, mutation
  invalidation, evidence persistence, received-evidence preservation, integrity
  versus correction status, incomplete-input and byte-consumption contract,
  resynchronization authority, and intentionally invalid encoding:
  `[required for protocol builders or parsers]`
- Hot paths, performance budgets, and allocation constraints: `[if applicable]`
- Queue capacities in items, worst-case retained bytes, queue-time assumptions,
  overload, backpressure, sample-discontinuity handling, selected queue
  implementation and wrapper responsibility, cancellation, and shutdown policy:
  `[if applicable]`
- Spawned-work lifecycle records—owner/handle, admission stop, shutdown signal,
  drain/discard, resource return, join deadline, timeout fallback, result/panic
  observer, and approved detachment: `[required when work is spawned]`
- Unsafe and FFI locations plus verification commands: `[if applicable]`

## Documentation, examples, and fixtures

- Required public/module documentation and vocabulary: `[if applicable]`
- Domain glossary location and local mappings for sample, chunk, block, dwell,
  receiver stages, received/recovered evidence, and public type names:
  `[required for DSP/receiver repositories]`
- Sample metadata placement: data-only finite buffer types, continuity-bearing
  boundary types, required metadata fields, and which stages establish or
  transform them: `[required for streaming repositories]`
- Discontinuities: stream epoch and index types, within-epoch known half-open or
  unknown loss extent, reason vocabulary, restart/reconfiguration behavior, and
  repeated-loss accumulation: `[required for lossy streaming repositories]`
- Processing composition: concrete/generic defaults, any closed runtime enums,
  approved shared trait boundaries, object-safe adapters, and dynamic-dispatch
  hot-path policy: `[required when processors are composed]`
- Rate relationships: directed rational vocabulary, absolute rates, checked
  current/reset/steady/final sizing APIs, latency/reference mapping, and any
  variable-rate exception: `[required for rate-changing stages]`
- Streaming completion: reset semantics, finite tail policy, completed-state
  behavior, live-stream finish/drain versus discard/reset ownership, and
  synthetic-tail provenance: `[required for stateful streaming stages]`
- Timing instrumentation: named events and capture points, clock domains,
  process-local monotonic values, exported durations, persistence/correlation
  policy, and hot-path sampling or aggregation: `[if timing is recorded]`
- Observability: typed events/snapshots/instruments, logging ecosystem,
  application-owned subscriber/exporter, instrument units and lifecycle,
  cardinality and sensitive-field bounds, optional adapters, and overhead
  budget: `[if operational diagnostics are exposed]`
- Example inventory and canonical invocation: `[if applicable]`
- Fixture provenance, storage, and regeneration: `[if applicable]`
- ADR, changelog, and generated-file rules: `[if applicable]`

## Local exceptions

List each exception with its exact scope, rationale, owner, and removal condition
when temporary. Omit this section when no exceptions exist.
