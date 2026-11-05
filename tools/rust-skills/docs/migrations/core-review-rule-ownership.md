# Core and Review Rule Ownership Map

Status: implemented; compatibility routers remain available for consumer migration

This map is the boundary contract for migrating the two compatibility packages.
It preserves every stable runtime rule ID while assigning one portable owner.
Routing is not ownership: a reviewing or implementing skill activates the named
owner instead of copying its directive.

## Core stable rules

| Existing rule | Portable owner | Routing or organization note |
|---|---|---|
| `CORE-DESIGN-001` | `rust-core` | Consequential-decision proportionality is universal judgment. |
| `CORE-DESIGN-002` | `rust-core` | Capability skills apply it to their own abstractions. |
| `CORE-DESIGN-003` | `rust-async-concurrency` | General caller ownership remains summarized in `rust-core`. |
| `CORE-API-001` | `rust-api-design` | Protocol and DSP skills supply domain invariants; R173 adds the durable trust-boundary transition. |
| `CORE-API-002` | `rust-api-design` | Compatibility checks route to dependency/security tooling where needed; R98 supplies conversion-trait semantics. |
| `CORE-API-003` | `rust-api-design` | Unsafe/FFI owns manual `Send`/`Sync` proof; dependency/security owns optional serialization integrations. |
| `CORE-API-004` | `rust-api-design` | Review checks coherence, collisions, sealing, preludes, and downstream compatibility. |
| `CORE-API-005` | `rust-api-design` | Review checks pointer semantics, coercion effects, mutable invariant exposure, and Borrow equivalence. |
| `CORE-API-006` | `rust-api-design` | Review checks caller evidence, conversion semantics, inference, generated-code cost, and parameter-form compatibility. |
| `CORE-API-007` | `rust-api-design` | Documentation owns declared textual compatibility; protocol owns machine grammar, vectors, and interoperability evidence. |
| `CORE-API-008` | `rust-api-design` | Protocol owns runtime unknown values; unsafe/FFI owns layout-sensitive consequences. |
| `CORE-API-009` | `rust-api-design` | Error policy owns whether an intentionally discarded failure remains observable; lint configuration owns diagnostic severity. |
| `CORE-OWN-001` | `rust-api-design` | Performance and async/concurrency own measured or cross-thread consequences. R167-R171 expand this boundary. |
| `CORE-OWN-002` | `rust-performance` | `rust-dsp` strengthens it for declared sustained DSP paths. |
| `CORE-OWN-003` | `rust-api-design` | Async/concurrency owns borrow-across-await and task-boundary consequences. |
| `CORE-ERR-001` | `rust-api-design` | Protocol and async/concurrency define domain error distinctions. |
| `CORE-ERR-002` | `rust-api-design` | Unsafe, protocol, embedded, and security profiles may strengthen it. |
| `CORE-ERR-003` | `rust-api-design` | Protocol and async/concurrency own domain distinctions; review detects shorthand that erases failures. |
| `CORE-ASYNC-001` | `rust-async-concurrency` | Review traces every losing branch; protocol owns framing and state-machine recovery consequences. |
| `CORE-ASYNC-002` | `rust-async-concurrency` | Dependency guidance owns feature truthfulness; applications may override ambient-runtime policy explicitly. |
| `CORE-ASYNC-003` | `rust-async-concurrency` | Repository onboarding records per-work-class policy; applications choose drain/discard and timeout escalation. |
| `CORE-CONC-001` | `rust-async-concurrency` | Performance owns grain-size evidence; DSP owns numerical and ordering equivalence. |
| `CORE-CONC-002` | `rust-async-concurrency` | Performance owns measured rate and latency evidence; repository onboarding records local budgets and exceptions. |
| `CORE-CONC-003` | `rust-async-concurrency` | Repository onboarding and dependency review select the concrete primitive; DSP owns sample-loss meaning and discontinuity propagation. |
| `CORE-DSP-001` | `rust-dsp` | Async/concurrency owns the dropping queue and overload policy; DSP owns state reset, metadata propagation, and reacquisition semantics. |
| `CORE-DSP-002` | `rust-dsp` | Repository onboarding records local mappings; protocol owns specification terms and received/recovered wire semantics; RSL organization decisions preserve exact libsdr names. |
| `CORE-DSP-003` | `rust-dsp` | Repository onboarding records metadata placement; async/concurrency owns transport loss and queue ordering; API design owns misuse-resistant associations. |
| `CORE-DSP-004` | `rust-dsp` | API design owns public trait evolution and sealing; performance owns measured dispatch cost; repository onboarding records local composition boundaries. |
| `CORE-DSP-005` | `rust-dsp` | API design owns public rate/bound types; performance owns sizing cost; onboarding records local absolute rates, latency mapping, and variable-rate exceptions. |
| `CORE-DSP-006` | `rust-dsp` | Async/concurrency owns the enclosing shutdown decision; protocol owns domain-specific incomplete-tail errors; onboarding records local tail and reuse policy. |
| `CORE-DSP-007` | `rust-dsp` | Async/concurrency owns queue and handoff events; performance owns measurement overhead; onboarding records clocks, capture points, export, and persistence policy. |
| `CORE-OBS-001` | `rust-core` | Dependencies own ecosystem adoption; applications own subscribers/exporters; performance owns overhead; domain skills own correctness-bearing evidence. |
| `CORE-PROTO-001` | `rust-protocol` | Core owns non-disableable safety/resource limits; API design owns builder ergonomics and policy evolution; security owns authentication trust boundaries. |
| `CORE-PROTO-002` | `rust-protocol` | API design owns message identity, validated forms, and mutation-safe invariants; security owns trust boundaries; onboarding records evidence retention and persistence policy. |
| `CORE-PROTO-003` | `rust-protocol` | DSP owns channel-quality consumption; API design owns evidence association and mutation safety; onboarding records received-evidence retention and constrained in-place exceptions. |
| `CORE-PROTO-004` | `rust-protocol` | API design owns public outcome ergonomics; async/concurrency owns surrounding transport delivery; onboarding records buffering, consumption, and resynchronization policy. |
| `CORE-PROTO-005` | `rust-protocol` | Security owns hostile-input threat analysis; onboarding records numeric budgets, scope, reset, rationale, and approved finite overrides. |
| `CORE-PROTO-006` | `rust-protocol` | API design owns public representation ergonomics; onboarding records repository policy for preserving or rejecting unknown and reserved values. |
| `CORE-SAFE-001` | `rust-unsafe-ffi` | Repository and organization unsafe policy decides whether activation is permitted. |
| `CORE-DEP-001` | `rust-dependencies-security` | RSL's preapproval threshold stays in its organization layer. |
| `CORE-DEP-002` | `rust-dependencies-security` | The `rsl-deps` preference moves wholly to the RSL organization layer. |
| `CORE-DEP-003` | `rust-dependencies-security` | Testing and embedded route here for feature matrices. |
| `CORE-DEP-004` | `rust-dependencies-security` | API design owns the exposed contract; async/concurrency owns runtime and pool semantics. |
| `CORE-CHANGE-001` | `rust-implement` | `rust-review` checks diff scope without restating the implementation workflow. |
| `CORE-CHANGE-002` | `rust-skill-maintenance` | Split version and generated-boundary checks from general task scope during migration. |
| `CORE-DOC-001` | `rust-api-design` | Domain skills provide protocol, safety, and lifecycle sections. |
| `CORE-DOC-002` | `rust-protocol` | `rust-api-design` owns ordinary public documentation shape. |
| `CORE-DOC-003` | `rust-api-design` | Repository onboarding records the local ADR threshold. |
| `CORE-EXAMPLE-001` | `rust-api-design` | `rust-testing` owns compilation and evidence for examples. |
| `CORE-EXAMPLE-002` | `rust-testing` | API and domain skills define production-shaped content. |
| `CORE-EXAMPLE-003` | `rust-api-design` | Performance and protocol skills identify costs and escape hatches. |
| `CORE-STYLE-001` | `rust-implement` | Domain enum exhaustiveness routes to API or protocol owners. |
| `CORE-STYLE-002` | `rust-implement` | Error policy remains owned by API design. |
| `CORE-STYLE-003` | `rust-implement` | Public vocabulary and domain types route to API design. |
| `CORE-STYLE-004` | `rust-api-design` | Async/concurrency and performance own sharing and cost implications. |
| `CORE-STYLE-005` | `rust-implement` | API design reviews public macro contracts; maintenance reviews generated content. |
| `CORE-STYLE-006` | `rust-core` | Unsafe explanations route to `rust-unsafe-ffi`; lint policy routes to dependency/security. |
| `CORE-TEST-001` | `rust-testing` | Every capability identifies the affected risk. |
| `CORE-TEST-002` | `rust-testing` | DSP and concurrency provide specialized determinism guidance. |
| `CORE-TEST-003` | `rust-testing` | Protocol owns conformance authorities and vectors. |
| `CORE-TEST-004` | `rust-testing` | Protocol, security, and async/concurrency define adversarial cases and bounds. |
| `CORE-TEST-005` | `rust-testing` | Performance owns benchmark interpretation. |
| `CORE-TEST-006` | `rust-testing` | Performance supplies the workload and metric; review blocks claims supported by invalid harness state. |
| `CORE-PERF-001` | `rust-performance` | Testing owns benchmark harness correctness and regression evidence. |
| `CORE-PERF-002` | `rust-performance` | Testing owns correctness evidence; repository profiles own artifact budgets and behavior-changing size settings. |

No stable ID is discarded. When a rule contains separable concerns, the
portable owner keeps the stable identity and links to routed subcontracts unless
the migration notes explicitly record an ID split.

## Review workflow ownership

| Existing review material | Owner | Migration behavior |
|---|---|---|
| Scope and precedence | `rust-review` | Retain and link to `rust-core` precedence. |
| Contract tracing | `rust-review` | Retain as review procedure; activate changed capability owners. |
| Evidence evaluation | `rust-review` | Retain reporting discipline; route command selection to `rust-testing`. |
| Severity and actionable-finding gate | `rust-review` | Retain as the sole owner of finding admission and format. |
| Public API and ownership checks | `rust-api-design` | Replace detailed duplication with activation questions and links. |
| Errors and hostile input checks | `rust-api-design` plus affected domain | Review retains only routing and consequence prioritization. |
| Concurrency and lifecycle checks | `rust-async-concurrency` | Review activates on relevant diffs. |
| Protocol and DSP checks | `rust-protocol` and `rust-dsp` | Each domain owns terminology, conformance, and evidence. |
| Unsafe, FFI, and performance checks | `rust-unsafe-ffi` and `rust-performance` | Review owns finding quality, not proof or benchmark content. |
| Tests, docs, examples, and scope checks | `rust-testing`, `rust-api-design`, and `rust-implement` | Review routes based on changed artifacts. |

## New decisions

- R98 was reaffirmed and strengthened: `From` requires infallible, semantically
  lossless, value-preserving, and obvious conversion; validation or failure uses
  `TryFrom`, consequential choices use named operations, and new code implements
  `From` rather than `Into` directly.
- R167 belongs to `rust-api-design`.
- R168 is an API ownership decision; `rust-performance` owns performance claims.
- R169 belongs only to the RSL organization layer.
- R170 is enforced by `rust-review`, with facts supplied by API and performance
  owners.
- R171 belongs to `rust-api-design`.
- R172 belongs to `rust-performance`; repository profiles approve
  behavior-changing or platform-specific size techniques.
- R173 belongs to `rust-api-design`; `rust-protocol` owns parsing-layer
  distinctions, unknown-value preservation, and malformed-data escape hatches.
- R174 belongs to `rust-testing`; `rust-review` enforces the blocking evidence
  consequence and `rust-performance` owns the declared workload and metric.
- R175 belongs to `rust-api-design`; manual `Send`/`Sync` implementations route
  to `rust-unsafe-ffi`, and optional serialization dependencies route to
  `rust-dependencies-security`.
- R176 belongs to `rust-api-design`; `rust-protocol` and
  `rust-async-concurrency` define failure distinctions and bounded observability
  for their domains.
- R177 belongs to `rust-api-design`; repository compatibility and representation
  boundaries override field-level extraction, while async/concurrency owns
  borrows retained across suspension.
- R178 belongs to `rust-async-concurrency`; `rust-review` traces losing branches,
  while `rust-protocol` owns reset and resynchronization behavior after partial
  wire progress.
- R179 belongs to `rust-api-design`; repository conventions decide whether a
  prelude exists, while `rust-review` checks public method and implementation
  compatibility.
- R180 belongs to `rust-api-design`; compatibility policy governs existing
  implementations, while `rust-review` checks implicit method and invariant
  exposure.
- R181 belongs to `rust-api-design`; `rust-performance` owns measured code-size
  or compile-time claims, while `rust-review` checks evidence for caller
  flexibility and compatibility.
- R182 belongs to `rust-api-design`, including declared human-facing format
  promises; `rust-protocol` owns canonical machine text and independent
  conformance evidence.
- R183 belongs to `rust-api-design`; `rust-protocol` owns runtime unknown values,
  while unsafe/FFI skills own layout-sensitive consequences.
- R184 belongs to `rust-api-design`; error policy owns the consequences of
  intentional failure discard, while repository lint policy owns warning
  severity.
- R185 belongs to `rust-dependencies-security`; API design owns the public
  surface, while async/concurrency owns runtime and pool coupling.
- R186 belongs to `rust-async-concurrency`; dependency guidance owns runtime
  feature naming, while application profiles may explicitly own ambient runtime
  policy.
- R109 and R110 remain owned jointly by `rust-async-concurrency` for pool
  ownership and `rust-performance` for threshold evidence; `rust-dsp` owns
  sequential/parallel numerical and ordering equivalence. Their wording now
  specifies `&rayon::ThreadPool`, `install`, a retained sequential path, and
  workload-specific size-sweep evidence.
- R26 remains owned by `rust-async-concurrency` and is strengthened from a
  qualitative bounded-queue preference to a required production data-path
  budget in items, retained bytes, and queue-time assumptions. Repository
  onboarding owns the local numbers; performance owns measured rate evidence.
- R28 and R105 remain owned by `rust-async-concurrency` for loss production and
  `rust-dsp` for stream meaning. Their wording now requires the next delivered
  buffer to carry its epoch, next absolute index, and known half-open or unknown
  within-epoch extent; separates reason from evidence; uses a new epoch for
  restart or reconfiguration; defaults stateful stages to reset; and covers
  checked counts, exact coalescing, rate changes, and reacquisition.
- R187 belongs to `rust-async-concurrency`; repository onboarding and dependency
  review own concrete queue selection. It rejects a globally blessed crate,
  records semantic mismatches locally, and centralizes nontrivial composite
  behavior behind a narrow domain queue type.
- R34 remains owned by `rust-async-concurrency` and now defaults production work
  to owned and joined. Repository onboarding records the work-class lifecycle;
  applications retain authority over drain/discard and deadline escalation.
- R188 belongs to `rust-dsp`, with protocol owning specification vocabulary and
  received/recovered wire distinctions. It resolves the generic buffer-name
  question with sample/chunk/block/dwell semantic defaults while repository
  glossaries retain precedence; RSL decisions preserve exact `libsdr` names.
- R189 belongs to `rust-dsp`, with API design owning type-level association and
  async/concurrency owning transport ordering and loss. It keeps finite storage
  data-only while requiring continuity-sensitive boundaries to bind only the
  metadata their stream claims need.
- R17, R101, and R190 jointly belong to `rust-dsp` for stage semantics and
  `rust-api-design` for public trait evolution. They reject a universal processor
  interface, retain static composition by default, and permit trait objects only
  for demonstrated runtime-open heterogeneity with complete contracts.
- R104 and R191 belong to `rust-dsp`; API design owns public ratio and bound
  types, while onboarding records absolute rates and variable-rate exceptions.
  Their wording fixes the direction to reduced `output/input`, requires checked
  state-aware bounds, and carries fractional phase across chunks.
- R102 and R192 belong to `rust-dsp`; async/concurrency owns the enclosing
  shutdown lifecycle, while protocol owns domain-specific finalization errors.
  Their wording separates reset from finite completion, requires one explicit
  tail policy and nonduplicating completed state, and preserves synthetic-tail
  provenance.
- R106 and R193 belong to `rust-dsp` for continuity separation and named
  streaming events; async/concurrency owns queue and handoff boundaries, while
  performance owns latency interpretation and instrumentation cost. Their
  wording keeps monotonic handles process-local, exports named durations, and
  keeps source, operational, and wall-clock domains distinct.
- R107, R108, R176, and R194 jointly establish the core observability contract.
  Dependencies own ecosystem review, applications own process-wide subscriber
  and exporter setup, and performance owns instrumentation cost. `tracing` is
  the recommended RSL structured-logging ecosystem but remains optional.
- R85 and R90-R93 belong to `rust-protocol`; core owns safety and finite
  resource limits, API design owns builder ergonomics and public policy
  evolution, and security owns authentication trust. Their wording makes the
  strict typed policy the default, requires independent named relaxations, and
  prohibits policy-based safety or resource-limit bypass.
- R91, R96, R195, and R196 belong to `rust-protocol` for the
  validation-evidence
  lifecycle. API design owns message identity, validated forms, and
  mutation-safe invariants; security owns trusted-input boundaries; onboarding
  records protocol-specific evidence retention and persistence; DSP owns
  channel-quality use of correction evidence. Their wording keeps ephemeral
  policy outside normal message identity, preserves exact received data apart
  from recovered output, keeps integrity observations distinct from correction
  outcomes, and prohibits stale evidence after mutation.
- R87 and R197 belong to `rust-protocol` for incremental parsing. API design
  owns public outcome ergonomics, async/concurrency owns surrounding delivery,
  and onboarding records buffering and recovery choices. Their wording
  distinguishes complete, incomplete, and malformed outcomes, prevents
  stateless overconsumption, and requires protocol evidence before discarding
  bytes to resynchronize.
- R85, R91, R93, and R198 belong to `rust-protocol` for parser resource
  budgets. Security owns hostile-input threat analysis and onboarding records
  numeric local choices. Their wording requires finite per-item and aggregate
  limits, checked enforcement before allocation or work, and no validity-policy
  or input-controlled bypass.

Runtime content has been migrated according to this map. The compatibility and
behavior changes are recorded in [`0.1-to-modular.md`](0.1-to-modular.md).
Future rule moves must update this map, the owning manifest, compatibility
notes, generated adapters, and relevant evaluations together.
