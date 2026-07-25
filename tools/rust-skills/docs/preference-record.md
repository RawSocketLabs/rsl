# Rust Engineering Preference Record

Status: discovery, research, and Stage 2A refinement complete; bounded Stage 3
implementation complete; independent eval acceptance and publication pending

This is the source of truth for the owner's engineering preferences while the
standards system is being designed. Rules remain drafts until their relevant
interview round is reviewed. An established preference is changed only through
an explicit revision recorded here.

Normative strengths:

- **MUST / MUST NOT**: required unless a documented, higher-precedence rule
  overrides it.
- **SHOULD / SHOULD NOT**: expected by default; deviations require a concrete
  reason.
- **PREFER**: choose this when relevant tradeoffs are otherwise comparable.
- **CONSIDER**: evaluate explicitly when the stated conditions apply.
- **MAY**: permitted, not required.

## Round 1: Engineering priorities and repository classes

### Confirmed preferences

#### Repository profiles

1. **Reusable SDR/DSP library**
   - Currently optimized around one consumer, Shrike, while intended for reuse
     by future applications.
   - Emphasize correctness, execution speed, and memory efficiency.
   - Domain mapping types are important.
   - Hot loops deserve stronger performance and allocation scrutiny than
     ordinary code.

2. **Protocol library**
   - Support rapid encoding and decoding of both valid and intentionally invalid
     protocol messages.
   - Emphasize flexible ergonomics over maximizing speed or memory efficiency.
   - Use types to explain the protocol and make ordinary incorrect operations
     difficult.
   - Provide easy, explicit escape hatches for work outside the normal
     specification.

3. **Applications**
   - Allow the repository to choose a performance-oriented or pragmatic,
     flexibility-oriented profile.
   - Keep business logic easy to read in CLI and long-running application code.

4. **Public reusable libraries**
   - Be robust, well documented, and hard to misuse.
   - Expose fewer consumer entry points.

5. **Internal libraries**
   - Permit more targeted implementations and require more domain understanding
     from their users.

6. **Experimental prototypes**
   - Prioritize flexibility.

#### Priority tiers

- **Top tier:** correctness, performance, abstraction quality, clarity.
- **Middle tier:** maintainability, simplicity, development velocity, security.
- **Lower tier:** compile time, binary size, API stability.

These tiers are not a blind ordering. Correct, simple, understandable use takes
priority over a faster or more abstract design that is difficult to understand
or easy to misuse.

#### Compatibility and change

- Breaking changes before `1.0.0` are acceptable.
- Conventional Commits should communicate compatibility and drive semantic
  versioning throughout development, including before `1.0.0`.

#### Performance posture

- Ordinary code should favor clarity until evidence or an explicit performance
  requirement identifies a hot path.
- Designated hot paths should be evaluated against relevant speed, latency,
  throughput, memory, or allocation requirements.

#### Agent autonomy

- When requirements are incomplete, make a conservative choice and continue if
  the choice is confined and inexpensive to revise.
- Ask before decisions that are difficult to reverse or have a large blast
  radius.

### Draft rules

#### R1. Preserve correctness

- **Strength:** MUST
- **Scope:** all Rust code
- **Rule:** Do not trade away correctness for performance, convenience, or
  abstraction quality.
- **Rationale:** correctness is top-tier and correct usage outranks speed.
- **Exceptions:** none identified.
- **Mechanical enforcement:** types, tests, property tests, fuzzing, and CI as
  appropriate; the exact profile remains unresolved.

#### R2. Make the engineering profile explicit

- **Strength:** SHOULD
- **Scope:** repository-local instructions
- **Rule:** State whether the repository or component follows a reusable-library,
  DSP/hot-path, protocol-flexibility, application-pragmatic, or prototype
  profile. Identify components that use a different profile.
- **Rationale:** the desired tradeoffs differ materially by repository and by
  component.
- **Exceptions:** a small repository whose profile is unambiguous from its local
  instructions.
- **Mechanical enforcement:** repository template validation may check for a
  declared profile.

#### R3. Make abstractions earn their complexity

- **Strength:** SHOULD
- **Scope:** all production code
- **Rule:** Prefer the design that remains clear, simple to use correctly, and
  difficult to misuse. Reject a more abstract design when it adds cognitive or
  misuse cost without a concrete benefit.
- **Rationale:** abstraction quality matters, but clarity and correct, simple use
  take precedence.
- **Acceptable exceptions:** measured performance requirements or domain
  invariants may justify additional complexity when the complexity is contained
  and documented.
- **Review questions:** What concrete failure or duplication does the
  abstraction prevent? Can users understand its ownership and invariants from
  the public surface?

#### R4. Minimize public library entry points

- **Strength:** SHOULD
- **Scope:** public reusable libraries
- **Rule:** Expose a small, robust, well-documented API that guides consumers
  toward correct use.
- **Rationale:** a smaller surface is easier to understand, document, test, and
  make hard to misuse.
- **Acceptable exceptions:** explicit low-level escape hatches with clearly
  different naming and documentation.
- **Mechanical enforcement:** API diff tooling may monitor surface growth, but
  judgment remains necessary.

#### R5. Require evidence for optimization complexity

- **Strength:** SHOULD
- **Scope:** production code; stronger in declared hot paths
- **Rule:** Favor clarity in ordinary code. Before accepting optimization-driven
  complexity, identify the requirement or evidence and describe how the change
  will be measured.
- **Rationale:** performance is top-tier, but unsupported optimization can harm
  clarity without improving the relevant workload.
- **Acceptable exceptions:** simple, established choices with no meaningful
  readability cost.
- **Mechanical enforcement:** benchmark and profiling requirements remain to be
  defined.

#### R6. Contain uncertain decisions

- **Strength:** MUST
- **Scope:** agent behavior
- **Rule:** When choosing under uncertainty, confine the decision behind a narrow
  boundary and state the assumption. Ask the owner before a broad or
  difficult-to-reverse architectural choice.
- **Rationale:** conservative progress is preferred only while future change
  remains inexpensive.
- **Review questions:** How many modules or consumers does this commit to the
  choice? Can the choice be replaced without changing the public API or stored
  data?

#### R7. Signal compatibility through commits

- **Strength:** SHOULD
- **Scope:** versioned repositories
- **Rule:** Use Conventional Commits, including explicit breaking-change signals,
  to drive semantic versioning throughout the development lifecycle.
- **Rationale:** API stability is a lower priority, but compatibility changes
  should remain deliberate and machine-identifiable.
- **Acceptable exceptions:** exploratory local commits that will be rewritten
  before integration.

#### R8. Permit deliberate pre-1.0 breakage

- **Strength:** MAY
- **Scope:** packages below `1.0.0`
- **Rule:** Make breaking changes when they materially improve the design; signal
  them explicitly rather than treating the pre-1.0 version as permission for
  silent churn.
- **Rationale:** early API correction is valuable, while consumers still need
  understandable change history.

### Scope distinctions and tensions

- **Performance versus clarity:** performance is top-tier, but it does not
  automatically justify complex code. DSP hot loops receive stronger scrutiny;
  protocol ergonomics and application readability may dominate elsewhere.
- **Abstraction quality versus simplicity:** abstraction quality means a design
  that clarifies the domain and prevents misuse, not maximum generality or type
  sophistication.
- **Low API stability versus disciplined change:** stability is not a primary
  constraint before `1.0.0`, but breakage should still be explicit and reflected
  in versioning.
- **Protocol validity versus escape hatches:** one builder should validate by
  default while allowing consumers to disable selected validations explicitly.
  This preserves a safe default without maintaining separate validated and raw
  construction models.
- **Reusable DSP library versus current Shrike focus:** near-term specialization
  is acceptable, but the amount of consumer-specific coupling permitted is
  unresolved.
- **Security:** security is middle-tier overall, but malformed or hostile input
  may require stronger protocol-specific rules. Input trust boundaries remain
  unresolved.

### Unresolved decisions

- Exact definitions and selection mechanism for repository/component profiles.
- Required benchmark or profiling evidence for hot-path changes.
- Whether performance budgets must be repository-local facts.
- How strongly public API documentation and semver checks are enforced before
  `1.0.0`.
- Trust boundaries and security posture for protocol parsers and applications.
- How much Shrike-specific design is acceptable in the reusable DSP library.
- Whether Conventional Commits are required for every commit or only integrated
  history.

## Round 2: APIs, ownership, and errors

### Confirmed preferences

#### Protocol construction and representation

- Use a builder pattern for message construction.
- Enable validation by default.
- Allow a consumer to disable selected validation aspects through the builder so
  intentionally invalid or nonstandard messages remain easy to construct.
- Prefer owned decoded protocol values over borrowed packet views as the primary
  representation.

#### Domain modeling

- Use newtypes and enums for domain concepts such as units, identifiers,
  bitfields, and protocol states.
- Prefer runtime validation when it communicates the rule more simply.
- Reserve typestate for costly misuse that a simpler builder, enum, or validated
  constructor cannot prevent clearly.

#### DSP API preference order

From most to least preferred:

1. An allocation-conscious low-level API with ergonomic allocating adapters.
2. Ownership transfer of reusable `Vec<T>` values or buffer objects.
3. Iterators that hide storage details.
4. Borrowed input paired with caller-provided output buffers.

#### Cloning and shared ownership

- Prefer borrowing or ownership transfer when natural.
- Cloning inexpensive values, or cloning to materially improve control-flow
  clarity, is acceptable.
- Do not clone buffers merely to avoid resolving the ownership design.
- Apply stronger scrutiny to allocation and buffer cloning in declared hot
  loops.
- Introduce `Arc`, `Cow`, `Bytes`, pools, arenas, or small-vector optimizations
  only for a concrete ownership or performance benefit.

#### Dispatch

- Prefer static dispatch most of the time, particularly in DSP code.
- Third-party DSP implementations are not an important design objective.
- Trait objects are permitted, but must justify their runtime and conceptual
  cost through a concrete composition or boundary requirement.

#### Errors and panics

- Public libraries return typed, inspectable domain errors.
- Protocol errors identify the operation, failure kind, and relevant field or
  offset without requiring string parsing.
- Applications may use `anyhow` or `eyre` at orchestration boundaries while
  preserving typed errors within domain components.
- Malformed or untrusted input must not cause a panic.
- `unwrap` is acceptable in tests when failure should fail the test.
- `expect` is acceptable during application startup for truly mandatory
  configuration when its message is actionable.
- Production libraries should be designed not to panic. A panic is acceptable
  only under an extreme condition that should be exceptionally difficult to
  reach.

### Draft rules

#### R9. Validate protocol construction by default

- **Strength:** MUST
- **Scope:** protocol message builders
- **Rule:** Enable all applicable protocol validation for ordinary builder use.
- **Rationale:** correct construction should be the path of least resistance.
- **Acceptable exceptions:** an explicitly disabled validation aspect.
- **Review questions:** Does the ordinary builder reject invalid field values and
  invalid cross-field relationships? Can a consumer tell which guarantees apply
  to the result?

#### R10. Make validation opt-outs explicit and granular

- **Strength:** MUST
- **Scope:** protocol message builders
- **Rule:** Permit intentional invalid construction by disabling named validation
  aspects. Do not require a separate raw construction model merely to bypass a
  check, and do not use one ambiguous switch when independent checks matter.
- **Rationale:** protocol testing and experimentation require malformed messages,
  while safe defaults protect ordinary consumers.
- **Acceptable exceptions:** a single switch may control a genuinely indivisible
  group of validations.
- **Review questions:** Is bypassing a check visibly intentional at the call
  site? Can unrelated checks remain enabled?

#### R11. Represent important domain concepts in types

- **Strength:** SHOULD
- **Scope:** libraries and domain-heavy application components
- **Rule:** Use newtypes, enums, and validated constructors when they explain the
  domain, prevent unit confusion, constrain values, or make invalid operations
  harder to express.
- **Rationale:** types should help readers understand DSP and protocol concepts
  and guide consumers toward correct use.
- **Acceptable exceptions:** a wrapper adds no semantic distinction or makes
  common operations substantially less clear.
- **Mechanical enforcement:** primarily review-based; exhaustive enum matching
  and visibility restrictions can preserve invariants.

#### R12. Reserve typestate for high-value invariants

- **Strength:** SHOULD
- **Scope:** public and internal APIs
- **Rule:** Use typestate only when compile-time state transitions prevent costly
  misuse more clearly than a builder, enum, or runtime validation step.
- **Rationale:** type-level state can prevent errors but can also spread generic
  complexity through consumers.
- **Acceptable exceptions:** none beyond a demonstrated improvement in safety and
  usability.

#### R13. Layer DSP APIs around an allocation-conscious core

- **Strength:** SHOULD
- **Scope:** reusable DSP libraries
- **Rule:** Provide a clear allocation-conscious core and build convenient,
  potentially allocating adapters on top. Make allocation behavior discoverable.
- **Rationale:** hot paths need control over memory behavior, while applications
  still need ergonomic entry points.
- **Acceptable exceptions:** an operation for which allocation is unavoidable or
  demonstrably irrelevant.
- **Review questions:** Can a hot-loop consumer reuse storage? Can a pragmatic
  consumer use the operation without manually managing every buffer?

#### R14. Prefer transfer of reusable buffers

- **Strength:** PREFER
- **Scope:** DSP pipelines and other buffer-oriented processing
- **Rule:** Transfer ownership of reusable buffers when it keeps allocation reuse
  explicit and avoids shared mutable ownership.
- **Rationale:** moving `Vec<T>` or a buffer object transfers its allocation
  without copying the elements.
- **Acceptable exceptions:** borrowing or shared ownership expresses the actual
  lifetime more clearly, or measurement does not justify reuse machinery.

#### R15. Resolve ownership before cloning buffers

- **Strength:** SHOULD NOT
- **Scope:** production code
- **Rule:** Do not clone a buffer merely to bypass an unresolved ownership or
  lifetime design. First evaluate borrowing, ownership transfer, or legitimate
  shared ownership.
- **Rationale:** buffer clones can conceal both architectural ambiguity and
  avoidable hot-path cost.
- **Acceptable exceptions:** the consumer requires an independent snapshot, or a
  clone materially simplifies a non-hot path with acceptable cost.

#### R16. Justify specialized storage and sharing types

- **Strength:** SHOULD
- **Scope:** production code
- **Rule:** Introduce `Arc`, `Cow`, `Bytes`, pools, arenas, small-vector storage,
  or similar machinery only for a concrete ownership, interoperability, or
  measured performance need.
- **Rationale:** each type adds semantic and operational complexity.

#### R17. Prefer static dispatch

- **Strength:** PREFER
- **Scope:** reusable libraries; stronger in DSP kernels
- **Rule:** Use generics or concrete types when the implementation set is known
  and static composition remains clear. Use trait objects only when runtime
  heterogeneity, object-safe boundaries, compile-time isolation, or another
  concrete need earns the indirection.
- **Rationale:** static dispatch aligns with DSP performance priorities and
  third-party DSP implementations are not a primary goal.
- **Acceptable exceptions:** a trait object materially simplifies application
  composition or isolates a boundary without harming the relevant performance
  path. A runtime-configurable DSP or audio pipeline may use a heterogeneous
  trait-object stage collection when runtime reordering, plugins, or genuinely
  open-ended stage types are requirements. Prefer concrete composition or an
  enum when the stage set is closed, and keep dynamic dispatch outside measured
  kernels where practical. Any shared processing trait must satisfy R101 and
  R190 rather than erasing unlike stage contracts.

#### R18. Return typed library errors

- **Strength:** SHOULD
- **Scope:** reusable libraries
- **Rule:** Return structured, inspectable error types that preserve relevant
  sources and domain details. Do not require consumers to parse display text.
- **Rationale:** callers need to diagnose, test, and sometimes react to failures.
- **Acceptable exceptions:** an infallible API or a deliberately opaque internal
  boundary whose callers cannot act on finer distinctions.

#### R19. Preserve protocol failure location

- **Strength:** SHOULD
- **Scope:** protocol encoding, decoding, and validation
- **Rule:** Include the operation, failure kind, and relevant field, byte offset,
  or bit offset when that information is available and meaningful.
- **Rationale:** malformed input and specification disagreements must be
  diagnosable without parsing prose.

#### R20. Use opaque application errors at orchestration boundaries

- **Strength:** MAY
- **Scope:** binaries and application orchestration
- **Rule:** Use `anyhow` or `eyre` where the caller will add context and report or
  terminate rather than branch on the concrete error. Preserve typed domain
  errors below that boundary.
- **Rationale:** application composition benefits from convenient context without
  weakening reusable library contracts.

#### R21. Keep reachable failure paths non-panicking

- **Strength:** MUST
- **Scope:** production libraries
- **Rule:** Return an error for malformed input, environmental failure, resource
  exhaustion that can be handled, invalid consumer data, and other reachable
  failures. Do not use panics as an ordinary error mechanism.
- **Rationale:** library consumers must control failure policy.
- **Acceptable exceptions:** an extreme internal invariant failure for which no
  valid recovery path exists and which should be exceptionally difficult to
  reach.
- **Review questions:** Could any external input or ordinary API use reach the
  panic? Can the invariant be represented or checked earlier? Would returning an
  error preserve useful behavior?

#### R22. Scope `unwrap` and `expect` narrowly

- **Strength:** SHOULD
- **Scope:** all Rust code
- **Rule:** Use `unwrap` freely in tests when failure should fail the test. Use an
  actionable `expect` for mandatory application startup configuration when
  termination is the intended policy. Avoid both in production library paths
  unless the extreme invariant exception in R21 applies.
- **Rationale:** convenience is appropriate when panic is explicitly the desired
  outcome, but library callers should otherwise retain control.

### Scope distinctions and tensions

- **One flexible protocol builder versus hard validity types:** domain types
  should prevent accidental misuse, but the builder must also construct invalid
  messages intentionally. Validation configuration therefore belongs to the
  construction process rather than requiring all values to be intrinsically
  valid.
- **Owned protocol values versus zero-copy parsing:** ownership and ergonomic
  flexibility currently outrank zero-copy lifetime complexity in the protocol
  library. Borrowed views may still be justified for a measured path.
- **Allocation-conscious core versus caller-provided output:** memory control is
  important, but raw output-slice APIs are the least preferred interface.
  Ownership transfer and ergonomic adapters should carry most use cases.
- **Static dispatch versus abstraction quality:** traits remain useful design
  tools, but dynamic dispatch needs a boundary-specific reason and should not be
  introduced solely for hypothetical third-party implementations.
- **Panic-free intent versus impossible guarantees:** dependencies and resource
  exhaustion may still abort or panic outside the library's direct control. The
  standard should govern reachable paths controlled by the project and require
  extreme invariant failures to be rare.

### Unresolved decisions

- Which protocol validation aspects must be independently configurable and how
  opt-outs should appear in builder method names.
- How owned protocol values preserve unknown fields and exact wire
  representations when round-tripping matters.
- How prominently allocating DSP adapters must signal allocation behavior.
- Whether reusable buffers use plain `Vec<T>`, a domain buffer newtype, or both.
- When an iterator API remains useful beside buffer-oriented DSP APIs.
- Criteria for choosing enums, generics, or trait objects at configurable
  application boundaries.
- Error enum stability, retryability metadata, source preservation, and crate
  choices such as `thiserror`.
- The exact set of extreme internal invariants that may panic and their
  documentation requirements.

## Round 3: Concurrency and streaming

### Confirmed preferences

#### Library execution model

- Reusable DSP libraries expose synchronous processing primitives as the core
  API.
- Optional features may add async/Tokio and parallel/Rayon integrations.
- Tokio or Rayon integration should be enabled when the consuming application
  already uses that ecosystem; the core library should not impose either one.
- In mixed applications, use Tokio for network I/O, timers, control flow, and
  orchestration; use explicitly owned worker threads for sustained CPU-bound DSP.
- Reserve `spawn_blocking` for bounded or occasional blocking work rather than
  permanent DSP loops.

#### Queues and overload

- Use bounded channels by default for async and threaded pipelines.
- Streaming sample overload drops the oldest queued samples, preserving fresher
  data.
- Coalesce control and configuration updates to the latest relevant value.
- Apply backpressure on reusable-buffer return paths.
- Protocol-message overload policy remains to be selected by transport and
  message semantics.
- Define the required queue semantics centrally, but select the concrete channel
  or queue implementation per repository through normal dependency review.
- Put nontrivial overload, recycling, and lifecycle behavior behind a small
  domain queue type instead of scattering ad hoc send/receive logic.

#### Buffer recycling

- Prefer moving buffer ownership through a bounded work queue and returning
  processed buffers through a bounded recycle queue.
- When the recycle pool is exhausted, apply backpressure rather than allocating
  replacement buffers by default.

#### Shared state

- Prefer a single owner plus message passing for evolving pipeline state.
- `Arc<Mutex<T>>` is acceptable for genuinely shared state with small critical
  sections.
- In async code, a standard synchronous mutex is acceptable for brief critical
  sections that never cross `.await`.
- Use an async mutex only when asynchronous acquisition or holding access across
  `.await` is genuinely required.
- Use atomics and lock-free structures only for simple invariants or measured
  contention, and document ordering assumptions.

#### Lifecycle and shutdown

- Production tasks and threads are owned and joined by default; detached work
  requires an explicit, bounded process-lifetime or external-supervisor policy.
- Repositories record shutdown, joining, draining, discard, buffer return,
  timeout, and escalation behavior per class of spawned work rather than
  inheriting one universal drain policy.

### Draft rules

#### R23. Keep the reusable DSP core synchronous

- **Strength:** MUST
- **Scope:** reusable DSP libraries
- **Rule:** Expose synchronous processing primitives that do not require an async
  runtime or parallel execution framework.
- **Rationale:** synchronous kernels remain portable, composable, testable, and
  usable by applications with different orchestration choices.
- **Acceptable exceptions:** none for the core API; opt-in adapters may supplement
  it.

#### R24. Put executor integrations behind opt-in features

- **Strength:** SHOULD
- **Scope:** reusable libraries
- **Rule:** Offer Tokio and Rayon integration only as explicit optional features,
  disabled by default. Do not create a hidden runtime or silently dictate the
  application's executor or thread-pool ownership.
- **Rationale:** framework integration is useful when the application already
  uses that framework, but it should not burden other consumers.
- **Acceptable exceptions:** an application-specific crate whose stated purpose
  requires the framework.
- **Mechanical enforcement:** Cargo feature and dependency inspection can verify
  that framework dependencies are optional and excluded from default features.

#### R25. Separate I/O orchestration from sustained DSP compute

- **Strength:** SHOULD
- **Scope:** applications combining async I/O and CPU-bound DSP
- **Rule:** Use Tokio for I/O and orchestration, and explicitly owned worker
  threads for sustained DSP loops. Use `spawn_blocking` for bounded or occasional
  blocking work rather than permanent compute loops.
- **Rationale:** sustained compute can starve executor workers, while dedicated
  threads make scheduling and lifecycle policy explicit.
- **Acceptable exceptions:** measurement shows that a bounded async-compatible
  execution strategy satisfies scheduling and latency requirements.

#### R26. Bound pipeline queues

- **Strength:** MUST
- **Scope:** async and threaded production pipelines
- **Rule:** Use bounded channels. Give each production data-path capacity a named
  unit and derive it from a declared burst, throughput, memory, or latency
  requirement. Record the limit in items and translate it into worst-case
  retained bytes and queue-time implications at the declared rates. Do not use a
  universal capacity value across unrelated repositories or pipelines.
- **Memory accounting:** Include variable-size owned payloads, shared backing
  allocations retained by queued handles, queue metadata, reserved permits, and
  other material in-flight storage. If item size is not bounded, enforce a byte
  or payload limit or cite the separate invariant that bounds it.
- **Time accounting:** State the assumptions behind the calculation. Minimum
  service rate bounds backlog drain and FIFO waiting; arrival and service rates
  together bound overload fill time. If consumers can stall indefinitely,
  capacity alone does not establish a finite queue-time guarantee.
- **Pipeline accounting:** Consider the sum of material queue and buffer-pool
  budgets along an end-to-end path. Validate user-configurable capacities
  against the repository's resource constraints.
- **Rationale:** bounded queues expose overload and place a limit on memory and
  queued work, but the bound is meaningful only when related to payload size,
  rates, and the other buffers in the path.
- **Acceptable exceptions:** low-volume control traffic whose possible growth is
  demonstrably bounded by another invariant; record that invariant near channel
  creation. Zero-capacity rendezvous and externally bounded queues should cite
  their actual bounding mechanism.
- **Review questions:** What happens when the queue fills? How much memory and
  delay can the configured capacity retain under the stated assumptions? What
  finite guarantee disappears when a consumer stalls?
- **Mechanical enforcement:** configuration validation, below/at/above-capacity
  tests, deterministic overload tests, occupancy and full-event metrics, and
  load tests where declared rates matter.

#### R27. Define overload behavior explicitly

- **Strength:** MUST
- **Scope:** every bounded production queue
- **Rule:** Define whether full capacity causes backpressure, oldest-item drop,
  newest-item rejection, coalescing, disconnection, or another explicit result.
  Make overload observable when lost or delayed work matters.
- **Rationale:** a bound without a full-queue policy leaves correctness and
  latency behavior undefined.
- **Mechanical enforcement:** review and queue-wrapper APIs may require a policy;
  metrics can expose full-queue events.

#### R28. Preserve freshness for streaming samples

- **Strength:** PREFER
- **Scope:** real-time or near-real-time sample queues
- **Rule:** When overload requires loss, discard the oldest queued samples so the
  consumer works on fresher data. Attach a discontinuity to the next delivered
  buffer with at least the known missing sample count or half-open index range
  and the absolute index of the next sample. Coalesce multiple drops before the
  next delivery without losing the total known loss.
- **Units:** Define whether indices count per-channel samples, interleaved scalar
  values, sample frames, symbols, or another domain unit, including origin and
  wrap/restart behavior. Represent an unknown loss extent explicitly rather than
  fabricating precision.
- **Rationale:** stale samples increase end-to-end latency and may be less useful
  than current samples, while explicit sequence metadata prevents freshness from
  masquerading as continuity.
- **Acceptable exceptions:** an algorithm requires continuity or complete sample
  history; in that case apply backpressure or fail the stream explicitly.
- **Review questions:** How are discontinuities signaled to stateful DSP stages?
  Are dropped sample counts observable without replacing the per-buffer signal?

#### R29. Coalesce replaceable control updates

- **Strength:** PREFER
- **Scope:** control and configuration queues
- **Rule:** Coalesce updates to the newest applicable value when intermediate
  states have no required side effect.
- **Rationale:** replaying stale configuration increases latency without adding
  value.
- **Acceptable exceptions:** every transition is semantically meaningful or must
  be audited.

#### R30. Recycle buffers through ownership transfer

- **Strength:** PREFER
- **Scope:** allocation-conscious streaming pipelines
- **Rule:** Move buffers through a bounded work queue and return them through a
  bounded recycle path. Apply backpressure when the reusable pool is exhausted
  rather than allocating replacements by default.
- **Rationale:** this keeps ownership clear and bounds steady-state allocation and
  memory use.
- **Acceptable exceptions:** a documented latency policy chooses drop or bounded
  temporary allocation instead.

#### R31. Prefer single ownership for evolving state

- **Strength:** PREFER
- **Scope:** concurrent production code
- **Rule:** Give evolving state one owner and communicate through messages when
  that model remains clear. Use shared mutable ownership when the state is
  genuinely shared and message passing would obscure the operation.
- **Rationale:** single ownership localizes invariants and reduces lock coupling.

#### R32. Keep synchronous locks away from await points

- **Strength:** MUST
- **Scope:** async code
- **Rule:** Do not hold a synchronous mutex guard across `.await`. Use a
  synchronous mutex for brief non-awaiting critical sections; use an async mutex
  only when asynchronous access is actually required.
- **Rationale:** holding a blocking guard across suspension risks executor stalls,
  deadlock, and difficult latency behavior.
- **Mechanical enforcement:** Clippy can detect some guard-across-await cases;
  review remains necessary.

#### R33. Justify atomics and lock-free structures

- **Strength:** SHOULD
- **Scope:** concurrent production code
- **Rule:** Use atomics for simple, well-defined invariants or after measuring
  relevant lock contention. Document the invariant and memory-order reasoning.
  Use lock-free structures only when their complexity earns a measured or
  architectural benefit.
- **Rationale:** concurrency complexity can compromise correctness without
  improving the real bottleneck.

#### R34. Declare lifecycle policy locally

- **Strength:** MUST
- **Scope:** repositories or components that spawn tasks or threads
- **Rule:** Record a lifecycle contract for every class of spawned work: who
  starts and owns it; which task, thread, or supervisor handle represents it;
  how new admission stops; how cancellation or shutdown is signaled; whether
  queued and in-progress work drains or is discarded; how resources and buffers
  are returned; the join deadline; the timeout fallback; and who observes
  completion, returned errors, and panics.
- **Default sequence:** Keep production work owned and joinable. Stop admission,
  signal shutdown, apply the locally selected drain-or-discard policy, return
  owned resources, and wait for completion within a declared bound. Treat the
  timeout fallback as part of the contract: abort, detach, force-close a
  resource, terminate the process, or report an incomplete shutdown only when
  its consequences are explicit.
- **Local decisions:** Drain versus discard depends on the work's semantics.
  State whether accepted work promises completion, whether partial progress is
  resumable, what data may be lost or duplicated, and whether a deadline has
  precedence over graceful completion.
- **Detached work:** Do not detach production work by dropping or discarding its
  handle. An explicit long-lived supervisor may assume ownership. Truly
  unjoined process-lifetime or best-effort work requires repository approval,
  bounded resource use, observable failure policy, and documented process-exit
  behavior.
- **Rationale:** shutdown correctness depends on application semantics and cannot
  be selected safely by a universal drain rule. A consistent ownership and join
  default prevents work, failures, buffers, and partial side effects from
  silently outliving their public owner.
- **Verification:** Test clean shutdown, shutdown with queued and in-progress
  work, saturated return paths, worker error or panic, and a worker that misses
  its deadline. Assert admission closure, declared loss or completion, resource
  return, timeout escalation, join results, and absence of leaked background
  work without relying on timing sleeps.
- **Acceptable exceptions:** A scoped concurrency primitive may encode ownership
  and joining directly. Test-only fault tasks and process-terminating paths may
  use narrower handling when their lifetime and cleanup consequences are
  explicit.

### Protocol overload default

- For reliable ordered streams such as TCP, propagate backpressure by pausing
  reads or admission while bounded application queues are full. If overload must
  terminate, reject explicitly, send a protocol-defined overload response, or
  close the connection. Do not silently discard an accepted message because that
  violates reliable-stream expectations at the application boundary.
- For datagrams such as UDP, local backpressure cannot reliably propagate to the
  sender. Bound ingress storage, drop datagrams under overload, and count the
  loss. Choose oldest versus newest drop from message semantics: preserve
  freshness for replaceable state, preserve queued order for event-like messages.
- For request/response services, bound concurrent and queued work, then reject
  excess admission explicitly rather than accepting work into an unbounded
  backlog.
- For latest-state control messages, coalesce rather than queue every update.

### Scope distinctions and tensions

- **Framework adapters versus a framework-neutral core:** Tokio and Rayon support
  is valuable only as an opt-in integration layer. The core remains synchronous
  and does not own the application's runtime.
- **Drop-oldest samples versus DSP continuity:** freshness is preferred under
  overload, but stateful algorithms may require explicit discontinuity handling
  or a non-dropping policy.
- **Buffer backpressure versus end-to-end latency:** recycling bounds memory and
  allocation but can stall producers. Each application must align queue capacity
  and backpressure with its latency budget.
- **General concurrency rules versus shutdown semantics:** ownership must be
  explicit everywhere, but drain-versus-discard and join behavior remain local
  policy choices.

## Round 4: Performance, unsafe Rust, and FFI

### Confirmed preferences

#### Performance evidence and tooling

- Declared DSP hot loops should perform no heap allocation after initialization.
- Performance-driven complexity requires representative before-and-after
  evidence, including relevant throughput, latency, and allocation measurements.
- Record enough information about the workload, toolchain, and hardware for a
  human to interpret the evidence.
- Use Criterion, flamegraphs, allocation measurement, and other appropriate tools
  to identify and explain performance behavior.
- Performance-regression checks must be runnable locally, not available only in
  CI.
- Make results easy to elevate for human review.
- Examine algorithms, data movement, allocation, batching, and memory layout
  before SIMD, unsafe code, lock-free structures, or elaborate specialization.
- Reserve aggressive techniques for explicit features or demonstrated hot paths
  where simpler approaches cannot meet the performance objective.
- Keep a clear scalar implementation as a correctness reference when practical.

#### Numerical and architecture policy

- Prefer `f32` for DSP unless precision, range, accumulation error, or another
  demonstrated requirement calls for `f64`.
- Select a set of CPU architectures for first-class correctness, performance,
  optimization, and CI support.
- Keep a correct scalar fallback for other supported architectures and document
  that they have not been optimized.
- For floating-point SIMD, compare with the scalar reference using a documented
  numerical contract rather than requiring bit-for-bit identity by default.
- Require exact behavior for integer, fixed-point, bit-oriented, and protocol
  operations unless the specification explicitly permits another result.

#### Unsafe Rust

- Deny unsafe code by default.
- Allow unsafe only in explicitly designated, narrowly scoped locations.
- Every unsafe operation states the invariant that makes it sound.
- Contain unsafe behind a safe abstraction where possible.
- Unsafe introduced for performance requires evidence that the safe approach
  cannot meet the documented objective.
- Apply all relevant verification methods, including Miri, sanitizers, fuzzing,
  property tests, and platform-specific CI.

#### FFI

- Keep FFI in narrow boundary modules.
- Validate pointer and length contracts.
- Document ownership, lifetime, allocation/deallocation, thread, and aliasing
  rules.
- Do not allow unwinding to cross an ABI boundary.
- Expose a safe Rust wrapper where possible.
- Use applicable Miri, sanitizer, fuzz, property, and target-platform checks.

### Draft rules

#### R35. Apply transport-specific overload semantics

- **Strength:** SHOULD
- **Scope:** networked applications
- **Rule:** Backpressure reliable ordered streams and reject sustained excess
  admission explicitly. Bound datagram ingress, drop under overload, and observe
  loss. Bound request concurrency and queues, rejecting excess work instead of
  accepting an unbounded backlog. Coalesce latest-state control updates.
- **Rationale:** reliable transports, datagrams, requests, and replaceable state
  have different delivery promises.
- **Acceptable exceptions:** a protocol specification or repository-local
  requirement defines a different explicit behavior.

#### R36. Avoid steady-state allocation in DSP hot loops

- **Strength:** SHOULD NOT
- **Scope:** declared DSP hot loops
- **Rule:** Do not allocate from the heap after initialization. Reuse or transfer
  preallocated buffers and state.
- **Rationale:** steady-state allocation adds latency variability, memory traffic,
  and allocator contention in the most performance-sensitive paths.
- **Acceptable exceptions:** measurement shows the allocation is irrelevant to
  the stated objective, or a documented operation cannot reasonably avoid it.
- **Mechanical enforcement:** allocation-counting tests or benchmarks can verify
  steady-state behavior.

#### R37. Measure performance-driven complexity

- **Strength:** MUST
- **Scope:** changes that add meaningful complexity for performance
- **Rule:** Provide a representative before-and-after benchmark and measure the
  resource claimed to improve. Record workload, toolchain, hardware, and enough
  methodology for human review.
- **Rationale:** complexity is acceptable only when it improves the relevant
  workload and helps meet a stated objective.
- **Acceptable exceptions:** a simple, established improvement with no meaningful
  readability or maintenance cost still requires an accurate explanation but
  may not warrant a dedicated benchmark.
- **Review questions:** Is the benchmark representative? Is the result larger
  than expected noise? Did another important metric regress?

#### R38. Keep performance investigations locally reproducible

- **Strength:** MUST
- **Scope:** repositories with performance-regression tooling
- **Rule:** Provide documented local commands for the benchmarks, profiling, and
  regression checks used by CI or human review.
- **Rationale:** developers and agents must be able to reproduce a regression and
  validate a proposed improvement before integration.
- **Mechanical enforcement:** validation scripts may verify that commands exist;
  benchmark execution may remain optional in ordinary CI due to noise and cost.

#### R39. Diagnose before escalating optimization techniques

- **Strength:** SHOULD
- **Scope:** performance work
- **Rule:** Evaluate algorithms, data flow, copies, allocation, batching, cache
  behavior, and memory layout before adding SIMD, unsafe, lock-free, or highly
  specialized implementations.
- **Rationale:** simpler changes often deliver larger improvements with less
  correctness and maintenance risk.
- **Acceptable exceptions:** the bottleneck and required technique are already
  demonstrated by reliable evidence.

#### R40. Contain aggressive optimization

- **Strength:** MUST
- **Scope:** SIMD, unsafe, lock-free, and elaborate specialized implementations
- **Rule:** Restrict the technique to an explicit feature or demonstrated hot
  path, preserve a clear boundary, and explain why simpler approaches cannot meet
  the objective.
- **Rationale:** specialized complexity should not spread through ordinary code.

#### R41. Preserve a scalar correctness reference

- **Strength:** SHOULD
- **Scope:** optimized DSP kernels
- **Rule:** Maintain a clear scalar implementation or reference model against
  which optimized implementations can be tested.
- **Rationale:** an independent, understandable reference makes SIMD and
  architecture-specific validation substantially stronger.
- **Acceptable exceptions:** maintaining two implementations would itself create
  unacceptable correctness risk; use another authoritative reference instead.

#### R42. Prefer `f32` unless precision evidence requires `f64`

- **Strength:** PREFER
- **Scope:** DSP numeric code
- **Rule:** Begin with `f32`. Use `f64` where error analysis, dynamic range,
  accumulation length, a specification, interoperability, or measurement shows
  that `f32` is insufficient.
- **Rationale:** `f32` typically offers better storage density and SIMD width,
  while `f64` should serve an identified numerical need.

#### R43. Define numerical equivalence per kernel

- **Strength:** MUST
- **Scope:** optimized numeric implementations
- **Rule:** State the correctness contract used to compare optimized and
  reference results. Require exact results for integer, fixed-point, bit, and
  protocol operations. For floating-point DSP, use justified absolute and
  relative tolerances, ULP bounds, or domain-level quality metrics. Require
  bitwise reproducibility only when it is an explicit product requirement.
- **Rationale:** SIMD reassociation, fused operations, and architecture-specific
  instructions can change rounding without making a result incorrect.
- **Review questions:** Does the tolerance scale correctly near zero and at large
  magnitudes? Does it conceal accumulated or unstable error? Are NaN, infinity,
  denormal, and boundary behaviors covered where relevant?

#### R44. Provide correct architecture fallbacks

- **Strength:** MUST
- **Scope:** architecture-optimized libraries
- **Rule:** Provide a correct portable or scalar path for supported targets that
  lack an optimized implementation. Detect optional CPU capabilities safely and
  document which architectures receive first-class optimization and CI coverage.
- **Rationale:** lack of optimization must not imply lack of correctness.
- **Acceptable exceptions:** a crate explicitly supports only a declared target
  architecture.

#### R45. Deny unsafe code by default

- **Strength:** MUST
- **Scope:** workspace and crate lint policy
- **Rule:** Deny unsafe code globally and allow it only at explicitly designated,
  narrowly scoped modules or items.
- **Rationale:** exceptional unsafe use should remain visible and reviewable.
- **Mechanical enforcement:** `unsafe_code` lint configuration plus validation of
  scoped allowances.

#### R46. Document and contain unsafe invariants

- **Strength:** MUST
- **Scope:** all project-controlled unsafe code
- **Rule:** State why every unsafe operation satisfies its safety contract.
  Document module-level invariants, keep the unsafe surface narrow, and expose a
  safe abstraction whenever possible. Document `# Safety` obligations for public
  unsafe APIs and unsafe traits.
- **Rationale:** soundness depends on invariants the compiler cannot verify.
- **Review questions:** Who establishes each invariant? For how long must it hold?
  Can safe callers violate it? Is the unsafe operation smaller than necessary?

#### R47. Require evidence for performance-motivated unsafe

- **Strength:** MUST
- **Scope:** unsafe code justified by performance
- **Rule:** Demonstrate that a safe implementation does not meet the stated
  objective and that the unsafe implementation materially improves the relevant
  measurement.
- **Rationale:** speculative performance does not justify soundness risk.

#### R48. Verify unsafe code with applicable dynamic tools

- **Strength:** MUST
- **Scope:** crates containing unsafe or FFI code
- **Rule:** Use all applicable checks from Miri, sanitizers, fuzzing, property
  tests, targeted invariant tests, and platform-specific CI. Document why a tool
  is inapplicable when an expected check cannot run.
- **Rationale:** no single tool covers all aliasing, lifetime, concurrency,
  boundary, and platform failure modes.
- **Mechanical enforcement:** dedicated local commands and CI jobs.

#### R49. Isolate and harden FFI boundaries

- **Strength:** MUST
- **Scope:** FFI code
- **Rule:** Keep foreign declarations and conversions in narrow modules; validate
  pointers, lengths, discriminants, and ownership transitions; document
  allocation, lifetime, aliasing, and thread rules; prevent unwinding across the
  ABI; and expose a safe Rust wrapper when possible.
- **Rationale:** FFI invalidates many Rust compiler guarantees at the boundary.
- **Acceptable exceptions:** a deliberately raw bindings crate may expose unsafe
  declarations directly, but higher-level consumers should use a separate safe
  wrapper.

### Numerical recommendation

For floating-point DSP, bitwise equality is not the general default. SIMD may
legitimately change rounding through reassociation, vector reduction order, or
fused multiply-add. Each kernel should define a numerical contract using the
smallest suitable combination of:

- absolute tolerance near zero;
- relative tolerance across ordinary magnitudes;
- ULP bounds where the operation has predictable rounding behavior;
- domain metrics such as SNR, error-vector magnitude, phase error, or filter
  response bounds for end-to-end signal behavior.

Test edge cases explicitly, including NaN, infinity, signed zero, denormals, and
range boundaries when they can occur. Require bitwise reproducibility only for a
stated cross-platform or persistence need. Integer, fixed-point, bitfield, CRC,
and protocol results remain exact unless their defining specification says
otherwise.

### Scope distinctions and tensions

- **No allocation versus ergonomic adapters:** the steady-state hot core avoids
  allocation; explicitly convenient adapters may allocate outside that core.
- **Performance regression CI versus noise:** checks must run locally and produce
  reviewable evidence, but automatic pass/fail thresholds may need controlled
  runners or manual interpretation.
- **Scalar reference versus duplicated maintenance:** a reference implementation
  improves optimized-code validation, but it should remain simple and
  authoritative rather than becoming a second optimized implementation.
- **Cross-architecture equivalence versus floating-point behavior:** first-class
  architectures share a documented numerical contract, not necessarily bitwise
  output.
- **Unsafe denied versus unsafe permitted:** denial is the default visibility
  mechanism, not an absolute ban. Narrow exceptions require stronger evidence,
  documentation, and testing.

### Unresolved decisions

- Minimum CPU baselines and feature-detection policy for the confirmed
  first-class Linux targets and Apple Silicon macOS.
- Exact benchmark metadata format and where reviewed results are stored.
- Whether performance thresholds run automatically in CI, on controlled runners,
  or as human-reviewed reports.
- Standard commands and tooling for allocation measurement, Criterion, and
  flamegraph capture.
- Default floating-point tolerance patterns for common DSP kernel categories.
- Policy for runtime CPU feature detection versus compile-time target features.
- Whether architecture-specific optimization is enabled automatically or through
  explicit Cargo features.
- How workspace-wide unsafe denial and scoped allowances will be validated.
- Minimum dynamic-test matrix for pure unsafe Rust versus FFI and concurrent
  unsafe code.

## Round 5: Testing and documentation

### Confirmed preferences

#### First-class platforms

- Treat `x86_64` and `aarch64` as first-class architecture families.
- Target Linux for the LattePanda Sigma, Jetson Nano, and Raspberry Pi systems.
- Treat Apple Silicon macOS as the first-class Mac target for development and
  deployment.
- Keep Intel macOS correct when practical, but do not require architecture-
  specific performance optimization unless a repository explicitly adopts it.
- Provide first-class correctness and relevant optimization coverage for this
  matrix; retain correct fallbacks for supported but unoptimized targets.

#### Test portfolio

- Use unit tests for local behavior and edge cases.
- Use integration tests through public APIs.
- Compile and test rustdoc examples.
- Use property tests for broad invariants.
- Fuzz parsers, unsafe boundaries, and complex decoders.
- Use authoritative specification or reference vectors when available.
- Select test layers according to the risk rather than requiring every test type
  for every function.

#### Protocol testing

- Test that default builders reject invalid values.
- Test that each validation opt-out disables only its intended checks.
- Test that intentionally invalid messages encode as requested.
- Ensure decoders do not panic on arbitrary input.
- Preserve and test unknown values where forward compatibility matters.
- Test parse/encode round trips for all semantically relevant information.
- Use fuzz and property testing for framing, length validation, malformed fields,
  and other broad input spaces.

#### DSP testing

- Use deterministic synthetic signals, authoritative golden vectors, and
  representative captured signals.
- Compare scalar and optimized implementations.
- Exercise varying chunk boundaries, alignment, empty and short buffers,
  discontinuities, and relevant numerical edge cases.
- Apply the kernel's documented numerical contract.

#### Concurrent-code testing

- Prefer deterministic coordination over sleeps and timing assumptions.
- Use Loom or an equivalent model checker for difficult concurrency invariants
  when practical.
- Keep long-running soak and performance tests separate from ordinary unit
  tests.

#### Test organization and fixtures

- Keep unit tests close to their implementation.
- Put public behavior and component interactions in integration tests.
- Prefer small fakes or in-memory implementations over elaborate mocking
  frameworks.
- Use captured data when realism matters, while preserving provenance and
  minimizing fixtures.
- Use snapshots only when their representation is stable and human-reviewable.

#### Documentation

- Document public library APIs.
- Document `# Errors` for public fallible APIs, `# Panics` for any permitted panic,
  and `# Safety` for unsafe APIs.
- Use module documentation to explain purpose, concepts, important invariants,
  data flow, and consistent domain vocabulary.
- Document why and invariants in complex private code rather than narrating the
  syntax.
- Cite exact specification sections for protocol behavior.
- Record important architectural tradeoffs in short design notes or ADRs.
- Provide guides in module documentation and other appropriate locations.
- Make libraries approachable for beginners without making expert use
  cumbersome.
- Deny missing public documentation in CI, with deliberate exceptions for
  generated code and intentionally raw binding crates.

### Draft rules

#### R50. Maintain a declared first-class platform matrix

- **Strength:** MUST
- **Scope:** reusable DSP, protocol, unsafe, and FFI libraries
- **Rule:** Declare first-class architecture and operating-system targets. Cover
  their correctness in CI and cover architecture-specific optimized paths where
  relevant. The initial families are `x86_64` and `aarch64`, targeting Linux and
  macOS.
- **Rationale:** the actual deployment systems include x86 Linux, ARM Linux, and
  Mac machines; optimization and unsafe correctness are target-sensitive.
- **Acceptable exceptions:** expensive performance tests may run on controlled or
  locally available target machines rather than every pull request.

#### R51. Match test evidence to risk

- **Strength:** MUST
- **Scope:** production libraries
- **Rule:** Select appropriate evidence from unit, integration, doc, property,
  fuzz, reference-vector, concurrency, and performance tests. Do not treat one
  test layer as sufficient for every risk.
- **Rationale:** public behavior, large input spaces, unsafe invariants, numerical
  behavior, and concurrency each fail in different ways.
- **Review questions:** What property could this change violate? Which test layer
  can demonstrate that property most directly?

#### R52. Test public behavior through public APIs

- **Strength:** SHOULD
- **Scope:** reusable libraries
- **Rule:** Keep focused unit tests near implementations and use integration
  tests to verify public contracts and component interactions without relying on
  private details. Unit tests in the same module MAY exercise private details.
  Do not make an item public only so a test can reach it unless that item is a
  legitimate reusable conformance-test interface.
- **Rationale:** public-surface tests catch accidental coupling and document how
  consumers use the library.

#### R53. Test protocol validity controls independently

- **Strength:** MUST
- **Scope:** configurable protocol builders and encoders
- **Rule:** Demonstrate that validation is enabled by default, each opt-out
  bypasses only its named checks, unrelated checks remain active, and the encoder
  emits intentionally invalid representations as requested.
- **Rationale:** granular escape hatches are useful only if their safety boundary
  is precise and stable.

#### R54. Make parsers panic-free under arbitrary input

- **Strength:** MUST
- **Scope:** protocol framing, parsing, and decoding
- **Rule:** Test arbitrary and malformed input with fuzzing, property tests, and
  targeted boundaries. Validate lengths before indexing or allocation and return
  structured errors instead of panicking.
- **Rationale:** parsers operate across a broad, potentially hostile input space.
- **Mechanical enforcement:** persistent fuzz targets plus regression fixtures
  for every discovered failure.

#### R55. Preserve protocol round-trip semantics

- **Strength:** SHOULD
- **Scope:** protocol models and codecs
- **Rule:** Test parse/encode round trips for all information the library promises
  to preserve, including unknown values when forward compatibility matters.
- **Rationale:** owned ergonomic models must not silently discard wire meaning.
- **Acceptable exceptions:** explicitly normalized or lossy representations whose
  documentation identifies the discarded information.

#### R56. Validate DSP across representations and boundaries

- **Strength:** MUST
- **Scope:** DSP kernels and streaming stages
- **Rule:** Combine deterministic synthetic inputs, authoritative golden vectors,
  and representative captured signals as applicable. Compare optimized paths to
  the scalar reference under the documented numerical contract and exercise
  chunk, alignment, length, discontinuity, and numerical boundaries.
- **Rationale:** DSP failures often depend on streaming shape and numeric edge
  behavior rather than only nominal samples.

#### R57. Make tests reproducible

- **Strength:** MUST
- **Scope:** automated tests
- **Rule:** Control random seeds, clocks, scheduling hooks, fixtures, and other
  nondeterminism where practical. Report the seed or reproducer for generated
  failures. Treat flakiness as a defect: a blind retry MUST NOT convert a
  failing result into acceptance. A temporary quarantine MUST identify an
  issue, owner, and removal condition.
- **Rationale:** a failure that cannot be reproduced is difficult to diagnose and
  unsafe as a release gate.
- **Acceptable exceptions:** deliberate soak or stress tests may explore real
  scheduling nondeterminism but must report enough context for investigation.

#### R58. Avoid timing-based concurrency assertions

- **Strength:** SHOULD NOT
- **Scope:** ordinary unit and integration tests
- **Rule:** Do not use arbitrary sleeps as the primary synchronization or
  correctness mechanism. Prefer barriers, channels, injected clocks, explicit
  state observation, or model checking.
- **Rationale:** timing assertions are slow and flaky and may still miss invalid
  interleavings.
- **Acceptable exceptions:** tests whose actual contract is a timeout or timing
  budget, using generous and platform-aware bounds.

#### R59. Model-check difficult concurrency invariants

- **Strength:** SHOULD
- **Scope:** custom synchronization, atomics, lock-free code, and subtle shutdown
  protocols
- **Rule:** Use Loom or an equivalent bounded model checker when it can represent
  the synchronization design. Keep soak and performance testing as separate
  complementary evidence.
- **Rationale:** ordinary tests sample very few interleavings.

#### R60. Prefer simple test doubles

- **Strength:** PREFER
- **Scope:** tests
- **Rule:** Use small fakes and in-memory implementations before elaborate mocks.
  Mock only behavior that must be observed at a narrow boundary.
- **Rationale:** behavior-heavy mocks often reproduce implementation structure
  and make refactoring unnecessarily difficult.

#### R61. Preserve fixture provenance

- **Strength:** MUST
- **Scope:** captured signals, packets, and other external test data
- **Rule:** Record the source, capture conditions, transformation, expected use,
  and redistribution rights needed to understand and maintain a fixture. Keep
  fixtures as small as the test permits. Record an integrity hash when it helps
  identify external or generated data, and provide an explicit regeneration
  command for generated fixtures.
- **Rationale:** unexplained captured data is difficult to validate, license, or
  regenerate.

#### R62. Use snapshots only for reviewable representations

- **Strength:** SHOULD
- **Scope:** tests
- **Rule:** Snapshot stable, meaningful representations whose diffs a reviewer
  can interpret. Do not use snapshots to hide large opaque changes or replace
  focused semantic assertions. Snapshot and golden-file regeneration MUST be an
  explicit developer action; CI MUST NOT accept changed output automatically.
- **Rationale:** snapshots are useful only when review can distinguish intended
  change from accidental churn.

#### R63. Document the public library surface

- **Strength:** MUST
- **Scope:** reusable public libraries
- **Rule:** Document public items and enforce missing-documentation checks. For
  fallible, panicking, and unsafe APIs, include applicable `# Errors`, `# Panics`,
  and `# Safety` sections.
- **Rationale:** documentation is part of a library's correctness and usability
  contract.
- **Acceptable exceptions:** generated code and deliberately raw bindings may use
  scoped lint exceptions with a documented reason.
- **Mechanical enforcement:** rustdoc tests and the `missing_docs` lint.

#### R64. Teach concepts and vocabulary in module documentation

- **Strength:** MUST
- **Scope:** domain-oriented library modules
- **Rule:** Explain the module's concepts, vocabulary, purpose, important
  invariants, and data flow. Use the same terms consistently across types,
  methods, errors, tests, and guides.
- **Rationale:** domain mapping types only help readers when the conceptual model
  is explicit and stable.

#### R65. Support progressive documentation depth

- **Strength:** SHOULD
- **Scope:** reusable libraries
- **Rule:** Provide concise entry-point examples and task-oriented guides for
  beginners, then expose detailed contracts and allocation-conscious or
  specialized APIs without forcing expert consumers through introductory
  wrappers.
- **Rationale:** the library should teach new users while remaining direct and
  non-cumbersome for experts.
- **Review questions:** Can a beginner complete a common task? Can an expert find
  precise ownership, allocation, numeric, and error behavior without reading a
  tutorial end to end?

#### R66. Cite defining protocol specifications precisely

- **Strength:** MUST
- **Scope:** protocol implementations and documentation
- **Rule:** Cite the defining document, version or revision, and exact section,
  table, or figure for behavior derived from a specification.
- **Rationale:** precise traceability makes implementation disputes and updates
  reviewable.

#### R67. Record consequential design tradeoffs

- **Strength:** SHOULD
- **Scope:** broad, durable, or difficult-to-reverse architectural choices
- **Rule:** Write a concise design note or ADR stating context, decision,
  alternatives, consequences, and evidence.
- **Rationale:** future maintainers should not have to reconstruct why a costly
  boundary or specialization exists.

### Stage 2A testing refinement

The owner confirmed the following additions in refinement round 1:

- Treat coverage as diagnostic evidence rather than a universal percentage
  gate. Permit focused thresholds and mutation testing when critical behavior
  justifies them.
- Allow unit tests to inspect private implementation details while keeping
  integration tests on public APIs. Do not expose production APIs solely for
  ordinary test access.
- Name tests for observable behavior and relevant conditions. Prefer one failure
  concept per test and table-driven cases for repeated behavior; do not mandate
  rigid Arrange/Act/Assert comments.
- Declare semantic `fast`, `default`, `extended`, `adversarial`, and
  `performance` tiers, then let each repository map applicable tiers to its own
  commands. Routine pull requests run the default tier.
- Test default features, meaningful no-default configurations, all features,
  and selected optional-feature interactions without blindly enumerating the
  complete power set. Exercise Tokio and Rayon integrations independently when
  they exist.
- Treat flakiness as a defect, prohibit blind retries as acceptance, report
  random seeds, and make any temporary quarantine owned and time-bounded by an
  explicit removal condition.
- Require deliberate golden and fixture regeneration, reviewable diffs, and
  provenance, licensing, and hashes where applicable. CI never auto-accepts an
  update.
- Assert structured error and domain semantics rather than incidental text or
  representation. Exact display text is asserted only when it is itself a
  contract.

#### R125. Use coverage as a diagnostic

- **Strength:** SHOULD
- **Scope:** automated test suites
- **Rule:** Use line and branch coverage to locate unexamined risk, not as a
  universal proof or repository-independent percentage target. A repository MAY
  set focused thresholds or apply mutation testing to critical parsers, state
  machines, and algorithms when the added signal justifies the cost.
- **Why:** High coverage can still miss incorrect properties, while a universal
  target encourages low-value assertions and implementation coupling.

#### R126. Name and structure tests around behavior

- **Strength:** SHOULD
- **Scope:** unit, integration, property, and regression tests
- **Rule:** Name a test for the behavior, condition, and expected outcome a
  failure would identify. Keep one failure concept per test and use table-driven
  cases when inputs share one behavior. Do not require ceremonial structure
  comments when the code is already clear.
- **Why:** A failing test should explain the broken contract without requiring a
  reader to reverse-engineer its setup.

#### R127. Publish semantic test tiers

- **Strength:** MUST
- **Scope:** repository testing instructions and CI
- **Rule:** Map each applicable semantic tier—`fast`, `default`, `extended`,
  `adversarial`, and `performance`—to canonical repository-local commands.
  Routine pull requests run the default tier. Keep expensive fuzz, soak,
  sanitizer, target-hardware, and benchmark work separately invokable locally
  and scheduled or reviewed according to repository risk.
- **Exception:** A repository MAY omit an irrelevant tier but MUST explain any
  material evidence that is not part of its default verification.

#### R128. Test meaningful feature configurations

- **Strength:** MUST
- **Scope:** crates with Cargo features
- **Rule:** Test default features, meaningful `--no-default-features`
  configurations, all features, and selected combinations where integrations
  can interact. Do not enumerate the complete feature power set without a
  concrete interaction risk. Test optional Tokio and Rayon integrations
  independently when offered.
- **Why:** Feature-gated code can rot independently, while exhaustive power-set
  testing becomes disproportionate quickly.

#### R129. Assert semantic contracts

- **Strength:** SHOULD
- **Scope:** automated tests
- **Rule:** Assert structured variants, fields, offsets, state transitions, and
  domain behavior. Avoid exact `Display`, debug, ordering, allocation, or other
  incidental representation assertions unless that representation is an
  explicit user-facing, protocol, or performance contract.
- **Why:** Tests should reject semantic regressions without freezing irrelevant
  implementation details.

#### R130. Preserve minimized regression cases

- **Strength:** MUST
- **Scope:** corrected correctness and security defects
- **Rule:** Add a focused regression test for every reproducible defect when the
  repository can exercise it. Minimize failures found by fuzzing or property
  testing and preserve them as deterministic regression cases. Name the test for
  the broken behavior; reference an issue or advisory only as supplementary
  context.
- **Exception:** If a practical automated reproducer cannot be retained, record
  why and identify the alternative evidence used to prevent recurrence.

#### R131. Share conformance suites across interchangeable implementations

- **Strength:** MUST
- **Scope:** implementations that promise the same behavioral contract
- **Rule:** Run one reusable conformance suite against scalar and optimized
  kernels, codec variants, backends, or other interchangeable implementations.
  Add implementation-specific tests only for their additional invariants.
- **Exception:** Keep the harness private unless consumers or third-party
  implementations genuinely need it as part of a supported contract.

#### R132. Test important compile-time misuse contracts

- **Strength:** SHOULD
- **Scope:** public APIs whose types intentionally reject invalid use
- **Rule:** Add compile-fail coverage for important static contracts. Prefer
  rustdoc `compile_fail` examples for small public cases. Introduce a dedicated
  diagnostic-testing dependency only when macros or a larger compile-test suite
  justify it, and avoid matching unstable compiler wording unnecessarily.
- **Why:** Misuse resistance is part of the API contract, but compiler diagnostics
  contain incidental details that should not be frozen without need.

#### R133. Keep performance evidence separate from functional timing

- **Strength:** MUST
- **Scope:** correctness tests and performance verification
- **Rule:** Ordinary correctness tests MUST NOT assert wall-clock performance
  unless time is part of the functional contract. Put throughput, latency, and
  allocation baselines in benchmarks or explicit performance tests. Enforce
  automated regression thresholds only on controlled runners; keep the same
  investigations locally runnable.
- **Exception:** Timeout and lifecycle tests MAY use generous, platform-aware
  bounds when expiration is the behavior under test.

#### R134. Make critical fuzzing continuous and reproducible

- **Strength:** MUST
- **Scope:** critical parsers, decoders, unsafe boundaries, and other declared
  adversarial surfaces
- **Rule:** Provide locally runnable fuzz targets and schedule CI campaigns at a
  repository-declared cadence. Commit small, high-value corpus seeds; store or
  cache large corpora outside the ordinary source tree. Minimize every discovered
  failure and retain its deterministic regression case.
- **Why:** Fuzzing must be repeatable enough for local diagnosis without allowing
  large evolving corpora to dominate the repository.

#### R135. Distinguish cross-compilation from native runtime evidence

- **Strength:** MUST
- **Scope:** declared first-class platforms
- **Rule:** Use cross-compilation to prove target buildability, but do not present
  it as runtime correctness evidence. Run native correctness tests across the
  first-class matrix and exercise architecture-specific optimized paths and
  performance on representative hardware before relevant releases.
- **Exception:** Document temporarily unavailable hardware and the resulting
  release risk rather than silently treating a build as an execution test.

#### R136. Approve testing dependencies through the normal policy

- **Strength:** MUST
- **Scope:** test, benchmark, fuzz, and compile-test dependencies
- **Rule:** Do not mandate one testing crate for every repository. Discuss and
  approve recurring defaults through `rsl-deps` when practical, while allowing a
  repository to choose a better-fitting tool with justification. Apply the same
  feature, MSRV, unsafe, maintenance, and supply-chain review used for production
  dependencies.
- **Why:** Development-only dependencies still affect the graph, toolchain, and
  maintenance surface, while different test risks need different tools.

#### R137. Test declared resource and lifecycle limits

- **Strength:** MUST
- **Scope:** bounded input, allocation, queue, backpressure, cancellation, and
  shutdown contracts
- **Rule:** Exercise behavior at, below, and beyond declared limits, including
  overload and shutdown interactions. Prefer generated inputs, injected budgets,
  deterministic clocks, and controllable schedulers over enormous checked-in
  fixtures.
- **Exception:** Retain a large captured fixture only when its realism proves a
  property that smaller or generated data cannot, and apply the provenance and
  storage policy.

### Stage 2A example refinement

The owner confirmed the following additions in refinement round 1:

- Give every target in `examples/` a concrete use case and distinguish its role
  as user-facing executable documentation from the role of integration tests.
- Use item rustdoc for focused calls, module rustdoc for concepts and workflows,
  and `examples/` for runnable multi-component use cases rather than copies of a
  complete consumer application.
- Compile and preferably run examples in CI. Reserve `no_run` for real execution
  constraints and `ignore` for exceptional, documented cases; exercise feature-
  gated examples under their declared features.
- Model fallible, non-panicking application code with `Result` and `?`; use
  `unwrap` or `expect` only when an intrinsic condition in a tiny example makes
  the choice clear and harmless.
- Teach the shortest correct common path first, then advanced ownership,
  validation, allocation, and performance controls without hiding material
  costs.
- Show normal protocol construction before clearly labeled validation escape
  hatches and intentionally invalid message construction.
- Keep primary DSP examples deterministic and hardware-independent. Put radio-
  specific setup in separate examples that make buffer, chunking,
  discontinuity, and performance behavior visible.
- Mechanically compile or derive every substantial example from compiled source;
  avoid both unverified duplication and opaque generation.

#### R138. Give every runnable example a specific use case

- **Strength:** MUST
- **Scope:** targets under `examples/`
- **Rule:** Give each example a task-oriented target name and top-level
  documentation stating the user task, material prerequisites, canonical
  invocation, expected behavior, and intentionally omitted concerns. Treat it as
  executable documentation, not as a miscellaneous binary or disguised
  integration test. A few sanity assertions MAY clarify the demonstrated
  invariant; edge-case and regression coverage belong in tests. Prefer
  deterministic output, and treat exact output as contractual only when
  explicitly documented.
- **Why:** An examples directory is useful only when each target answers a
  concrete consumer question and has a maintenance purpose distinct from tests.

#### R139. Put examples at the narrowest useful documentation layer

- **Strength:** SHOULD
- **Scope:** public library documentation
- **Rule:** Use item rustdoc for a focused API operation, module rustdoc for a
  conceptual workflow, and `examples/` for a runnable scenario spanning multiple
  components. Do not reproduce a full consumer application inside the library.
- **Why:** Matching example size to the task keeps discovery direct and prevents
  toy applications from becoming parallel products.

#### R140. Compile examples under their real configurations

- **Strength:** MUST
- **Scope:** rustdoc and `examples/` targets
- **Rule:** Compile and preferably execute examples in CI under their declared
  Cargo features. Use `no_run` only when execution requires hardware, networking,
  credentials, or disproportionate setup. Use `ignore` exceptionally and record
  why ordinary compilation is impossible.
- **Why:** Public examples are API consumers and should detect drift in the
  configurations they teach.

#### R141. Model fallible application code honestly

- **Strength:** SHOULD
- **Scope:** user-facing examples
- **Rule:** Prefer `Result`-returning entry points and `?` for fallible work. Use
  `unwrap` or `expect` only when the condition is intrinsic to a small example
  and cannot imply an acceptable production panic path.
- **Why:** Examples teach habits through imitation and should align with the
  library's non-panicking production policy.

#### R142. Teach progressive paths without hiding costs

- **Strength:** SHOULD
- **Scope:** beginner and performance-sensitive examples
- **Rule:** Lead with the shortest correct common path, then show advanced
  ownership, validation, allocation, or performance controls when they matter.
  Identify important allocation, copy, blocking, thread, runtime, and feature
  costs instead of concealing them behind pedagogical convenience.
- **Why:** Beginners need a clear entry point, while expert consumers need to
  understand the operational contract without reverse-engineering wrappers.

#### R143. Label protocol escape-hatch examples explicitly

- **Strength:** MUST
- **Scope:** protocol builders and encoders with optional validation
- **Rule:** Demonstrate valid, default construction first. Put validation opt-out
  and intentionally invalid message examples in clearly labeled scenarios that
  identify exactly which invariant is being bypassed and which checks remain.
- **Why:** Flexible protocol tooling should make advanced use easy without
  presenting unsafe or invalid construction as the normal path.

#### R144. Keep primary DSP examples deterministic and hardware-independent

- **Strength:** SHOULD
- **Scope:** DSP library examples
- **Rule:** Use deterministic synthetic data and domain types for primary
  examples. Isolate radio or platform setup in hardware-specific examples and
  make buffer ownership, chunking, discontinuities, and material performance
  behavior explicit there.
- **Why:** Core concepts should be runnable on ordinary development machines
  while hardware examples retain domain realism.

#### R145. Prevent example drift mechanically

- **Strength:** MUST
- **Scope:** substantial code in documentation and examples
- **Rule:** Compile the code directly or derive the displayed form from compiled
  source through a transparent, validated process. Prefer links or verified reuse
  over copied code, but do not introduce generation that makes the documentation
  harder to read or review.
- **Why:** An elegant example that no longer compiles is actively misleading,
  while opaque synchronization machinery undermines clarity.

The owner confirmed the following additional choices in refinement round 2:

- Put purpose, prerequisites, invocation, expected behavior, and intentional
  omissions at the example source; use task-oriented names.
- Permit small illustrative assertions while keeping exhaustive and regression
  evidence in tests. Prefer deterministic output without making incidental text
  contractual.
- Use real public APIs and production-shaped flows rather than test helpers,
  invented façades, or unresolved placeholders.
- Keep approved example-only dependencies in development scope when practical
  and gate examples explicitly instead of enlarging default library features.
- Maintain only examples with distinct scenarios and update them in the same
  change as affected APIs.
- Provide a canonical Cargo invocation and avoid adding a command-line framework
  solely for trivial argument parsing.
- Declare external requirements, fail actionably, clean up resources, and offer
  deterministic sample or simulation paths where practical.
- Keep benchmark harnesses and performance claims out of examples.

#### R146. Use production-shaped public APIs in examples

- **Strength:** MUST
- **Scope:** user-facing examples
- **Rule:** Demonstrate the real supported public API and representative
  ownership, error, and lifecycle flow. Do not depend on test-only helpers or
  invent an undocumented convenience façade. An example MAY omit orthogonal
  setup when the omission is explicit, but MUST NOT leave a `TODO` in place of
  behavior essential to the demonstrated use case.
- **Why:** Copyable code should lead consumers toward supported designs rather
  than an example-only architecture.

#### R147. Isolate example dependencies and feature requirements

- **Strength:** MUST
- **Scope:** example targets and Cargo configuration
- **Rule:** Apply normal dependency approval to example tooling and keep it in
  development scope when practical. Declare `required-features` or equivalent
  gating rather than expanding default library features solely to compile an
  example. Do not add a command-line framework for a simple example when direct
  argument handling remains clear.
- **Why:** Example convenience should not silently enlarge the consumer's normal
  dependency or feature surface.

#### R148. Maintain a distinct, current example inventory

- **Strength:** MUST
- **Scope:** public libraries with runnable examples
- **Rule:** Keep only examples that teach distinct consumer scenarios; impose no
  numeric quota. Consolidate or remove redundant targets. Update every affected
  example in the same change as the API or behavior it demonstrates.
- **Why:** A small purposeful inventory is more discoverable and maintainable
  than a growing collection of near-duplicate demos.

#### R149. Make external-resource examples actionable

- **Strength:** MUST
- **Scope:** examples using hardware, networks, files, credentials, or other
  acquired resources
- **Rule:** State requirements before use, report missing prerequisites with an
  actionable error, and clean up acquired resources. Provide deterministic
  sample data, simulation, or a dry path when practical without pretending it is
  equivalent to hardware validation.
- **Why:** Environment-specific examples should teach setup and lifecycle rather
  than fail mysteriously or leave side effects.

#### R150. Keep performance measurement out of examples

- **Strength:** SHOULD
- **Scope:** performance-oriented examples
- **Rule:** Examples MAY demonstrate allocation-conscious or optimized APIs but
  MUST NOT act as ad hoc benchmark harnesses or publish unverified speed claims.
  Put comparisons and regression evidence in the repository's benchmark and
  profiling workflow.
- **Why:** Example execution environments are uncontrolled and cannot support
  trustworthy performance conclusions.

### Stage 2A nonmechanical style refinement

The owner confirmed the following choices in refinement round 1:

- Prefer `match` for enums, `Option`, `Result`, multiple meaningful cases, and
  exhaustive state reasoning. Use `if` for direct boolean or numeric predicates
  and `if let` only when one pattern is genuinely the sole interesting case.
- Use early returns and `let ... else` to reject preconditions and keep the
  successful path flat.
- Use combinators for short, obvious transformations; prefer explicit `match`,
  loops, and named intermediates when business rules, errors, state, or branching
  would otherwise be hidden.
- Extract functions around coherent domain concepts and invariants rather than an
  arbitrary line-count target.
- Keep mutation narrowly scoped and use shadowing only for legible type, unit,
  validation, or ownership transitions.
- Use stable domain vocabulary, positive boolean names, and explicit units at
  primitive boundaries.
- Organize modules by domain concepts and capabilities, avoiding generic dumping
  grounds and accidental public re-exports.
- Prefer explicit imports, limiting globs and aliases to deliberate, locally
  understandable cases.

#### R151. Prefer `match` for structured branching

- **Strength:** PREFER
- **Scope:** branching code
- **Rule:** Use `match` for enums, `Option`, `Result`, multiple meaningful cases,
  and decisions where exhaustiveness documents the state space. Prefer it over
  chains of `if let` or `else if let`. Use `if` for straightforward boolean or
  numeric predicates, and use `if let` when exactly one pattern matters and the
  remainder is intentionally uninteresting.
- **Why:** `match` makes domain states and unhandled cases visible without adding
  ceremony to ordinary predicate checks.

#### R152. Flatten preconditions and preserve the successful path

- **Strength:** SHOULD
- **Scope:** functions with validation, optional input, or early failure
- **Rule:** Use guard clauses, early returns, and `let ... else` for invalid
  preconditions or required destructuring when doing so keeps the main flow
  flatter. Do not split a cohesive decision into many exits when one explicit
  `match` is clearer.
- **Why:** Business logic is easier to follow when error setup does not surround
  the successful path with indentation.

#### R153. Prefer explicit control flow when combinators obscure policy

- **Strength:** SHOULD
- **Scope:** iterator, `Option`, `Result`, and future-processing chains
- **Rule:** Use combinators for short transformations whose data flow is obvious.
  Switch to `match`, loops, and named intermediate values when branching, error
  context, state transitions, ownership, or side effects become difficult to
  read in the chain.
- **Why:** Concision is valuable only while the reader can still see the domain
  decision and failure path directly.

#### R154. Extract functions around concepts, not line counts

- **Strength:** SHOULD
- **Scope:** function and helper design
- **Rule:** Give a function one coherent domain purpose without enforcing an
  arbitrary maximum length. Extract a helper when its name clarifies a concept,
  contains an invariant, enables meaningful reuse or testing, or materially
  improves local reasoning. Do not fragment sequential logic solely to shorten
  the source.
- **Why:** Both oversized functions and fleets of trivial helpers can hide the
  actual business flow.

#### R155. Constrain mutation and meaningful shadowing

- **Strength:** SHOULD
- **Scope:** local bindings and state transitions
- **Rule:** Keep `mut` bindings in the smallest practical scope. Use shadowing
  when the same conceptual value advances through a clear type, unit, validation,
  or ownership transition. Use a new name when the meaning changes or repeated
  shadowing would make earlier and later values difficult to distinguish.
- **Why:** Local transformation should remain visible without forcing artificial
  names or allowing one mutable binding to accumulate unrelated meanings.

#### R156. Name domain meaning and units explicitly

- **Strength:** SHOULD
- **Scope:** identifiers and public vocabulary
- **Rule:** Reuse the repository's domain terms consistently. Prefer positive
  boolean names such as `is_valid`, `has_signal`, and `can_retry`. Encode units in
  domain types; when a primitive crosses a boundary, include the unit in the
  identifier, such as `sample_rate_hz`.
- **Why:** Names should let readers understand a rule without reconstructing
  negation, units, or synonyms from surrounding code.

#### R157. Organize modules around domain capabilities

- **Strength:** SHOULD
- **Scope:** module and crate layout
- **Rule:** Group code by domain concept, capability, or cohesive boundary rather
  than broad `utils`, `common`, or `helpers` buckets. Make public re-exports an
  intentional API layer and do not expose internal layout by accident.
- **Exception:** A narrowly scoped support module MAY use a generic name when its
  contents and ownership remain cohesive and locally obvious.

#### R158. Keep imports explicit and purposeful

- **Strength:** SHOULD
- **Scope:** `use` declarations
- **Rule:** Prefer explicit imports. Use glob imports only for deliberate preludes
  or tightly scoped contexts where the complete imported vocabulary is known.
  Alias a name only to resolve a collision or improve domain clarity, and keep a
  function-local import only when its narrow scope materially helps the reader.
- **Why:** Readers should be able to identify a name's origin without excessive
  qualification or hidden namespace expansion.

The owner confirmed the following additional choices in refinement round 2:

- Keep owned and internal enum matches explicit when new variants should force a
  decision; use fallbacks where external non-exhaustive or preserved unknown
  values require them.
- Use `?` for direct propagation and `match` for recovery, classification, or
  domain transformation; retain structured library errors.
- Use iterators for clear transformations and `for` loops for stateful, fallible,
  side-effecting, or interruptible work.
- Make cloning deliberate, using `Arc::clone` and `Rc::clone` to emphasize shared
  ownership and reconsidering clones added only to appease the borrow checker.
- Comment reasons and invariants rather than syntax, and make significant TODOs
  actionable and traceable.
- Use macros only when they provide significant value beyond functions, traits,
  and generics.
- Keep unsafe blocks minimal with adjacent concrete `SAFETY` reasoning, including
  inside unsafe functions.
- Default to private visibility and scope explained lint exceptions as narrowly
  as possible, preferring checked expectations when supported.

#### R159. Keep owned enum matches meaningfully exhaustive

- **Strength:** SHOULD
- **Scope:** matches over domain enums and state machines
- **Rule:** List meaningful variants explicitly when adding a variant should
  force reconsideration of the decision. Combine patterns only when their
  semantics are genuinely identical. Use a fallback for external
  `#[non_exhaustive]` types, intentionally preserved unknown values, or state
  spaces where an explicit catch-all is part of the contract.
- **Why:** Exhaustiveness is valuable when it exposes domain evolution, but a
  false enumeration is inappropriate when the domain intentionally remains open.

#### R160. Keep error propagation structured and visible

- **Strength:** SHOULD
- **Scope:** fallible code
- **Rule:** Use `?` for direct propagation. Use `match` when recovering,
  classifying, adding domain context, or intentionally translating an error.
  Keep `map_err` closures short and avoid reducing structured library errors to
  strings before a presentation boundary.
- **Why:** Error flow should remain concise without hiding policy or discarding
  information consumers need.

#### R161. Match iteration form to control flow

- **Strength:** PREFER
- **Scope:** collection and stream processing
- **Rule:** Use iterators for clear, side-effect-free transformations. Use `for`
  loops when processing is stateful, fallible, side-effecting, or clearer with
  `break` and `continue`. Avoid a dense `fold` for a complex state machine.
- **Why:** The chosen form should expose rather than compress the important
  control flow.

#### R162. Make cloning and shared ownership explicit

- **Strength:** SHOULD
- **Scope:** value duplication and reference-counted ownership
- **Rule:** Keep clones visible and intentional. Prefer `Arc::clone(&value)` and
  `Rc::clone(&value)` when the operation represents shared ownership; use
  `.clone()` for ordinary value duplication. Do not add a clone merely to satisfy
  the borrow checker without evaluating a clearer ownership or borrowing design.
- **Exception:** A measured hot path may require a specialized ownership choice;
  document and test its contract rather than hiding the cost.

#### R163. Comment durable reasons and make TODOs actionable

- **Strength:** SHOULD
- **Scope:** source comments
- **Rule:** Explain invariants, units, protocol authority, performance
  constraints, safety reasoning, and non-obvious decisions rather than narrating
  syntax. First improve names, types, functions, and control flow where they can
  express the behavior directly, but do not remove a durable explanation merely
  because code is described as self-documenting. Give a significant `TODO` or
  `FIXME` enough context plus a tracking reference or removal condition to make
  the deferred work actionable.
- **Why:** Comments should preserve information the code cannot express and
  should not become unowned wish lists.

#### R164. Require significant value from macros

- **Strength:** SHOULD
- **Scope:** declarative and procedural macros
- **Rule:** Prefer functions, traits, and generics when they express the design
  adequately. Introduce a macro only when syntax generation, meaningful
  repetition reduction, compile-time structure, or another concrete benefit is
  significant enough to justify harder navigation and diagnostics. Document
  nontrivial grammar, hygiene assumptions, and error behavior.
- **Why:** Macros can unlock important capabilities, but small convenience gains
  rarely repay their abstraction and tooling cost.

#### R165. Keep unsafe operations locally justified

- **Strength:** MUST
- **Scope:** unsafe operations and unsafe functions
- **Rule:** Scope each unsafe block tightly around the operations that require it,
  including within an `unsafe fn`. Put a concrete `SAFETY` explanation adjacent
  to the block and expose a safe wrapper whenever a sound reusable contract can
  be enforced.
- **Why:** Small, local proof obligations make unsafe review and later
  modification tractable.

#### R166. Minimize visibility and lint-exception scope

- **Strength:** SHOULD
- **Scope:** item visibility and lint attributes
- **Rule:** Default items to private and use the narrowest required
  `pub(super)`, `pub(crate)`, or `pub` visibility. Attach a lint exception to the
  smallest relevant item and explain it. Prefer `#[expect]` where supported so a
  suppression that stops matching becomes visible; use broader `allow` policy
  only when generation or conditional compilation requires it.
- **Why:** Narrow visibility preserves design freedom, while checked, local lint
  exceptions resist silent policy decay.

#### R167. Borrow the optional referent at API boundaries

- **Strength:** SHOULD
- **Scope:** new read-only function parameters and return values
- **Rule:** Represent an optional borrowed value as `Option<&T>` or
  `Option<&mut T>` rather than `&Option<T>`. Borrow the deepest useful referent,
  such as `Option<&str>` instead of `Option<&String>`. Use `as_ref`,
  `as_deref`, and their mutable forms when adapting stored optional values.
- **Why:** The API should expose the conceptual optional value rather than the
  owner's storage container. This accepts direct values, stored options, and
  absence without coupling the callee to `Option<T>`, and it lets the callee use
  `Option` combinators without repeatedly reborrowing the container.
- **Exceptions:** Use `&mut Option<T>` when the operation changes presence or
  transfers the contained value, such as `take` or replacement. Pinning,
  required trait or FFI signatures, compatibility, and a rare contract about
  the exact container may also require a container reference. Do not
  mechanically rewrite transient internal `&Option<T>` values. Treat a public
  signature migration as a compatibility change.

#### R168. Select frozen sequence ownership by required capability

- **Strength:** MUST
- **Scope:** material long-lived sequences whose shape is immutable after
  construction
- **Rule:** Explicitly choose among `Vec<T>`, `Box<[T]>`, `Rc<[T]>`, and
  `Arc<[T]>` from the required capabilities. Retain `Vec<T>` for building,
  mutation, growth, capacity reuse, or an API that requires it. Use
  `Box<[T]>` for unique frozen ownership, `Rc<[T]>` for shared single-threaded
  ownership, and `Arc<[T]>` for shared ownership that crosses threads or must
  satisfy `Send` and `Sync`.
- **Why:** These types communicate materially different mutation, sharing, and
  thread-safety contracts. The choice should follow the ownership topology
  rather than a blanket preference for one container.
- **Exceptions:** Small, short-lived, or non-material values do not need a
  design ceremony. Interoperability, construction cost, conversion cost,
  copy-on-write, or measured behavior may justify a different representation.

#### R169. Prefer shared frozen slices in RSL when shared cloning is the design

- **Strength:** PREFER
- **Scope:** Raw Socket Labs repositories and components with long-lived,
  immutable sequence data
- **Rule:** When multiple logical owners intentionally share the same immutable
  sequence, prefer `Rc<[T]>` within one thread and `Arc<[T]>` across threads.
  Prefer `Box<[T]>` when ownership remains unique. Preserve `Vec<T>` when
  mutation, capacity, or buffer reuse is part of the contract.
- **Why:** RSL protocol, DSP, and systems code frequently retains tables,
  coefficients, or other frozen data across processing objects. Cheap shared
  clones can express that topology without deep-copying the elements.
- **Exceptions:** Do not introduce reference counting merely because cloning
  exists. Independent snapshots, small values, short lifetimes, and measured
  reference-count overhead may favor `Vec<T>` or `Box<[T]>`.

#### R170. Require evidence before criticizing a sequence container

- **Strength:** MUST
- **Scope:** implementation and review findings about `Vec`, `Box`, `Rc`, or
  `Arc`
- **Rule:** Do not treat a `Vec<T>` or a clone as a defect without establishing
  the value lifetime, mutation needs, clone frequency, ownership topology,
  thread boundary, workload, and consequence. Performance changes require
  before-and-after evidence when performance is the justification.
- **Why:** Replacing a deep clone with reference counting changes independent
  ownership into sharing and introduces allocation, count-update, cycle, and
  lifetime tradeoffs. The type alone does not prove a defect.

#### R171. Avoid a redundant owned container beneath frozen shared ownership

- **Strength:** PREFER
- **Scope:** immutable shared strings and sequences
- **Rule:** Prefer `Rc<[T]>` or `Arc<[T]>` to `Rc<Vec<T>>` or `Arc<Vec<T>>`, and
  prefer `Rc<str>` or `Arc<str>` to reference-counting a `String`, when spare
  capacity, growth, and the inner owned-container API are not part of the
  contract.
- **Why:** The unsized form states the frozen-shape contract and avoids retaining
  an extra owned-container layer solely to reach its contents.
- **Exceptions:** Keep the inner `Vec<T>` or `String` when mutation through
  unique access, capacity, conversion behavior, or an external API is required
  and justified.

#### R172. Optimize binary size against a defined shipped artifact

- **Strength:** MUST
- **Scope:** executable, firmware, WebAssembly, package, and container-size work
- **Rule:** Define what is being minimized, including the artifact boundary,
  target triple, toolchain, features, profile, and whether the measurement is
  stripped, compressed, or packaged. Record reproducible before-and-after byte
  counts and preserve correctness tests. Start with stable, reversible profile
  choices and measure each one: release mode, symbol stripping, `"s"` versus
  `"z"` size optimization, LTO modes, and codegen units can have
  workload-specific results.
- **Why:** An unqualified “smaller binary” claim can compare different targets,
  features, debug information, or packaging. Size-oriented settings can also
  trade runtime performance, compile time, observability, portability, and
  operational behavior for fewer bytes.
- **Escalation:** Inspect contributors with target-appropriate tools such as
  `cargo-bloat`, `cargo-llvm-lines`, or Twiggy when available. Require an
  explicit repository decision before changing panic behavior, removing
  diagnostic or location information, requiring nightly `build-std`, replacing
  `std` with `no_std`, using `no_main`, changing linking or allocation strategy,
  packing the binary, or adding unsafe code. Document the lost capability and
  deployment or security consequence.
- **Exceptions:** Repositories without a material size requirement do not need
  to optimize or continuously gate artifact size. A constrained target may
  adopt aggressive settings as a profile decision after recording the required
  toolchain, platform, diagnostics, and validation.

#### R173. Parse durable trust-boundary values into invariant-preserving types

- **Strength:** SHOULD
- **Scope:** values that enter through public, wire, configuration, storage,
  FFI, or other trust boundaries and remain meaningful downstream
- **Rule:** Convert an untrusted representation once into a domain type whose
  normal constructors establish the relevant durable invariants. Keep its
  invariant-bearing fields private when practical, return a structured parsing
  failure, and let downstream APIs accept the parsed type instead of raw data
  plus repeated validation.
- **Why:** A successful parse changes what downstream code may assume. Carrying
  that fact in the type localizes validation, prevents accidental bypass, and
  removes duplicated checks that can drift apart.
- **Exceptions:** Keep raw or partially parsed representations when the value is
  transient, the check is local and inexpensive, lossless diagnostics or round
  trips require original data, unknown values must be preserved, or testing
  deliberately constructs malformed protocol data. Do not claim permanent
  validity for invariants that depend on mutable state, time, external
  resources, or a later application context. In protocol code, keep framing,
  structural parsing, integrity checking, semantic validation, and application
  interpretation distinguishable even when types carry results between layers.

#### R174. Block performance claims supported by invalid benchmark state

- **Strength:** MUST
- **Scope:** benchmarks used to justify or verify performance-motivated changes
- **Rule:** Make every measured sample exercise the declared representative
  workload. Regenerate or reset mutating and consuming input outside the timed
  operation, use the harness's appropriate setup or batching facility, and keep
  setup and destruction outside the measurement unless the named metric includes
  them. Verify enough input and output behavior to show that optimization or
  stale state did not remove the intended work.
- **Why:** A benchmark can silently measure a different operation after its
  first iteration, such as repeatedly sorting an already-sorted buffer. Precise
  timing of the wrong workload is not performance evidence.
- **Review consequence:** Treat a state-invalid benchmark as a blocking finding
  whenever it supports a performance claim or performance-motivated change.
  Withdraw the claim, repair the harness, or provide independent valid evidence
  before accepting the change.
- **Exceptions:** A cumulative or stateful benchmark may evolve state when that
  evolution is the documented production workload. An end-to-end metric may
  intentionally include construction, teardown, allocation, or I/O when those
  costs are named and consistently measured.

#### R175. Implement common capability traits only when their contracts are true

- **Strength:** MUST
- **Scope:** public and domain types
- **Rule:** Evaluate `Clone`, `Default`, `Serialize`, `Deserialize`, `Send`, and
  `Sync` from the type's semantics and consumers rather than treating them as a
  required checklist. `Clone` must produce the intended logical duplicate and
  may carry a documented material cost. `Default` must represent a valid,
  unsurprising baseline rather than inventing sentinel identifiers or invalid
  empty state. Serialization traits establish a data-format and compatibility
  surface; deserialization must not bypass the type's invariants.
- **Thread safety:** Let `Send` and `Sync` emerge from the actual fields and
  safety model. Do not replace `Rc` with `Arc`, add locks, or otherwise redesign
  ownership solely to force these auto traits. A genuine cross-thread
  requirement may justify such a redesign, but it must preserve the type's
  semantics and operational behavior. Any manual `unsafe impl Send` or
  `unsafe impl Sync` requires the full written safety argument and validation
  expected for unsafe code.
- **Why:** These traits grant capabilities and make promises to generic and
  downstream code. A convenient derive can create invalid values, expose a
  costly or surprising operation, freeze a wire/storage representation, or make
  an unsound concurrency assertion.
- **Exceptions:** Implement a trait when a concrete repository or consumer
  contract requires it and the design truthfully supports it. Feature-gate
  ecosystem integrations such as Serde when they are optional and preserve
  invariant checks through validated intermediates or custom implementations.

#### R176. Make intentional error loss explicit and observable

- **Strength:** MUST
- **Scope:** fallible iterators, parsers, streams, tasks, message delivery, and
  batch or record processing
- **Rule:** Propagate or handle failures by default. Do not silently turn a
  `Result` into absence through `Result::ok`, `filter_map`, `flatten`, ignored
  return values, or equivalent shorthand unless discarding that failure is an
  explicit part of the operation's contract. Name the behavior as best-effort,
  lossy, skip-invalid, or other repository vocabulary rather than hiding it in
  control flow.
- **Observability:** Expose suitable aggregate counts, structured diagnostics,
  quarantine output, a returned summary, or another repository-appropriate
  signal. Preserve incomplete, malformed, retryable, overload, cancellation, and
  shutdown distinctions when they imply different recovery. Avoid unbounded or
  per-item hot-path logging; aggregate at a controlled boundary.
- **Why:** Mapping errors to `None` can make corrupted input, data loss, closed
  consumers, or operational failures indistinguishable from an ordinary filter.
  Iterator concision does not justify erasing behavior callers or operators need.
- **Exceptions:** Deliberately lossy telemetry, probes, caches, sampling, and
  best-effort importers may discard individual failures when the consequence,
  scope, and observability policy are explicit and tested.

#### R177. Match borrows to the capability and scope actually required

- **Strength:** SHOULD
- **Scope:** function and method parameters, receivers, helpers, and returned
  borrows
- **Rule:** Express only the ownership and access capability the operation
  requires. Prefer shared access over mutable access, borrow the deepest useful
  referent, and accept `&[T]` or `&str` when ownership, growth, capacity, or the
  concrete container is irrelevant. Avoid retaining a borrow beyond the work
  that uses it.
- **Field-level composition:** Extract a helper over the affected field or
  component when a whole `&self` or `&mut self` receiver overstates access,
  prevents independent field borrows, or couples otherwise separable operations.
  Keep the higher-level method as an orchestration wrapper when that preserves a
  useful API.
- **Why:** Signatures are capability contracts. Narrow borrows make independent
  state easier to compose, reduce accidental mutation authority, and localize
  borrow-checker conflicts without cloning or shared-ownership escape hatches.
- **Exceptions:** Keep a whole-object receiver when an invariant spans multiple
  fields, representation hiding is important, a trait or compatible public API
  requires it, or extraction reduces clarity. Do not contort code merely to
  shorten a lexical borrow, expose private representation, or make an unmeasured
  compile-time claim.

#### R178. Analyze what cancellation drops at every race

- **Strength:** MUST
- **Scope:** `select` operations, timeouts, task abort, future races, dropped
  futures, and equivalent cancellation paths
- **Rule:** For every race, identify the losing operation and analyze what
  dropping it does at each suspension point. Account for partial I/O, consumed
  messages, external side effects, locks, permits, owned buffers, and protocol or
  application state. State whether restarting the operation is safe.
- **Non-resumable work:** If partial progress cannot be resumed without loss,
  duplication, or state corruption, move progress into durable owned state,
  complete or roll back before racing, or abandon and reset the affected
  connection, stream, or state machine. Do not assume encode/decode or framed I/O
  remains synchronized after a cancelled partial operation.
- **Task semantics:** Distinguish dropping a future from dropping a task handle.
  Verify the selected runtime's detach, abort, completion, and blocking-task
  behavior. Keep result, cleanup, and shutdown ownership explicit.
- **Verification:** Inject cancellation before and after relevant suspension
  points and during partial reads or writes. Assert resource return, observable
  errors, state-machine behavior, and whether work continues in the background.
- **Why:** Cancellation occurs at operation-specific suspension boundaries.
  Paired operations can appear correct on the happy path while a losing branch
  silently consumes bytes, loses a message, leaves background work running, or
  corrupts subsequent framing.
- **Exceptions:** Application shutdown may intentionally discard progress when
  the operation contract allows it and the associated resource and state are
  conclusively abandoned. A current library guarantee of cancellation safety may
  satisfy the restart question, but not task ownership or cleanup.

#### R179. Use extension traits only for a coherent foreign-type capability

- **Strength:** SHOULD
- **Scope:** traits that add callable methods to types the crate does not own
- **Rule:** Create an extension trait only when its methods form one named,
  reusable capability and method syntax materially improves use or generic
  composition. Prefer inherent methods for owned types, free functions for an
  isolated operation, and a newtype or wrapper when behavior needs distinct
  semantics, state, or invariants. Do not accumulate unrelated helpers in a
  miscellaneous `*Ext` trait.
- **Receiver and implementation scope:** Define the intended receiver set.
  Justify broad blanket implementations and check their coherence implications.
  Document whether downstream crates may implement the trait. Seal it, and
  document the sealing, when implementation must remain under the defining
  crate's control.
- **Compatibility:** Treat public extension methods, supertraits, required
  items, blanket implementations, and downstream implementation freedom as API
  commitments. Check proposed method names against inherent methods and likely
  traits in scope; collisions can cause ambiguity or silently select an inherent
  method. Provide a stable, discoverable import path.
- **Prelude policy:** A repository may expose a narrow, opt-in prelude containing
  a cohesive extension-trait vocabulary. A prelude is not permission to hide
  unrelated imports or export a growing convenience grab bag.
- **Verification:** Compile public consumer fixtures and documentation tests that
  import and call the trait as users will. Exercise fully qualified trait syntax
  when collision, generic inference, or receiver resolution is material, and run
  semantic-version checks where the repository requires them.
- **Exceptions:** Retain compatibility extension traits while migrating callers,
  and allow tightly scoped internal traits when their local capability remains
  obvious. Do not introduce a public trait merely to make a single helper look
  like a method.

#### R180. Reserve deref coercion for transparent pointer behavior

- **Strength:** SHOULD
- **Scope:** implementations of `Deref`, `DerefMut`, `AsRef`, `AsMut`, `Borrow`,
  and `BorrowMut`
- **Rule:** Implement `Deref` only when a wrapper transparently and cheaply acts
  as a stable target type and implicit coercion is unsurprising. Do not use
  deref coercion to simulate inheritance, expose a convenient inner field, or
  obtain another type's methods for an ordinary domain newtype.
- **Alternatives:** Prefer explicit domain methods when the wrapper has distinct
  semantics or invariants. Use `AsRef` for an infallible, cheap
  reference-to-reference conversion. Use `Borrow` only when the owned value and
  borrowed form have equivalent `Eq`, `Ord`, and `Hash` behavior; otherwise use
  `AsRef` or an explicit accessor.
- **Mutable access:** Implement `DerefMut`, `AsMut`, or `BorrowMut` only if every
  mutation available through the target preserves the wrapper's invariants and
  does not bypass required validation. Read-only transparency does not imply
  mutable transparency.
- **Compatibility:** Treat the target type and implicit target-method surface as
  public API. Changing or removing them can break callers, while wrapper methods
  can collide with target methods. Dereferencing should not perform surprising
  work or fail unexpectedly.
- **Verification:** Test invariant preservation through every exposed mutable
  conversion. Compile representative downstream coercions and method calls when
  adding or changing a public implementation.
- **Exceptions:** Purpose-built smart pointers, guards, storage wrappers, and
  transparent collection or string wrappers may implement deref traits when
  they satisfy the complete contract. Preserve a legacy public implementation
  when compatibility costs outweigh removal, but do not copy it into new APIs
  without fresh justification.

#### R181. Add generic input bounds for demonstrated flexibility

- **Strength:** SHOULD
- **Scope:** public function and constructor parameters
- **Rule:** Use a concrete parameter type or ordinary borrow when one
  representation is the real contract. Add `impl Trait` or a named generic
  parameter when its bound accurately states the capability the implementation
  needs and multiple natural caller forms provide a demonstrated ergonomic or
  compositional benefit. Do not add generic conversion bounds solely because a
  public API might someday need another input form.
- **Conversion contracts:** Use `AsRef` for an infallible, cheap borrowed
  conversion and `Into` only for an infallible consuming conversion. Use
  `TryInto` or a named conversion when failure or domain interpretation matters.
  Document ownership transfer and any allocation, normalization, or validation
  performed after conversion.
- **Capability exception:** Prefer a meaningful capability bound such as
  `IntoIterator` when iteration is genuinely the complete input requirement.
  This is a narrower semantic contract, not genericity for its own sake.
- **Costs:** Evaluate inference, call-site readability, the breadth of accepted
  implementations, code size, compile time, and optimization opportunity.
  Measure these costs when they decide the design rather than assuming every
  monomorphization is harmful. A thin generic adapter may normalize input and
  delegate to a concrete body to contain generated code.
- **Compatibility:** Treat the bound and parameter form as public API.
  Argument-position `impl Trait` is an anonymous generic parameter; switching to
  or from a named parameter can break callers that explicitly specify generic
  arguments. Adding bounds or narrowing accepted implementations can also break
  downstream code.
- **Verification:** Compile documentation tests and external consumer fixtures
  for each intended caller form, including ambiguous conversion and inference
  cases. Run semantic-version checks where required.
- **External-guidance tradeoff:** Rust API Guidelines recommend minimizing input
  assumptions with generics. This decision retains that advice for real
  capability boundaries while preferring a smaller concrete surface over
  speculative convenience.

#### R182. Separate human formatting from machine encoding

- **Strength:** MUST
- **Scope:** `Display`, `Debug`, `FromStr`, logs, persistence formats, and wire
  representations
- **Rule:** Use `Display` for the type's single obvious human-facing
  representation and `Debug` for programmer-facing diagnostics. Do not parse
  either output for program logic or silently treat it as a stable persistence,
  interchange, or protocol format. Use dedicated serialization or encoding APIs
  for machine contracts.
- **Stability:** Assume derived and dependency `Debug` output can change.
  `Display` may intentionally omit precision or state and is not lossless or
  stable unless its documentation makes that promise. Keep secrets and other
  sensitive fields out of both surfaces.
- **Round trips:** When `Display` is intentionally lossless and
  machine-parseable, document the grammar, normalization, and compatibility
  policy; make `FromStr` accept it; and test `value.to_string().parse()` across
  representative and boundary values. Having both traits does not imply they are
  inverses unless documented.
- **Multiple formats:** Use named display adapters or explicit formatters when a
  type has multiple useful textual representations rather than making one
  context-dependent `Display` implementation.
- **Structured behavior:** Return variants, fields, error codes, or dedicated
  encoded values when callers need machine-actionable data. Do not require
  callers, tests, metrics, or automation to parse presentation text.
- **Protocol evidence:** A specification may define a canonical textual machine
  representation. Treat it as a versioned protocol surface and test independent
  known-answer or interoperability examples because self-round trips can hide
  paired defects.
- **Exceptions:** Exact `Display` or `Debug` assertions are appropriate when the
  representation is the explicitly declared contract or when a focused test is
  verifying redaction. Otherwise assert semantic structure.

#### R183. Apply `#[non_exhaustive]` selectively for intended source evolution

- **Strength:** SHOULD
- **Scope:** public enums, structs, and enum variants
- **Rule:** Use `#[non_exhaustive]` when future variants or fields are an
  intended public evolution path, preferably from the item's first release. Do
  not use it as a blanket marker for all public types. Keep genuinely closed
  domain sets exhaustive, and keep matches within the defining crate
  meaningfully exhaustive so local additions require deliberate handling.
- **Compatibility:** The attribute restricts downstream construction and
  exhaustive matching, so adding it to an existing public item can itself be a
  breaking source change. Adding a variant to an already non-exhaustive enum can
  preserve downstream source compatibility, but behavior, layout, FFI,
  serialization, and generated representations still require review.
- **Unknown values:** `#[non_exhaustive]` is not a runtime unknown-value
  mechanism. Protocols and durable formats that must preserve unrecognized
  values should use an explicit raw-preserving representation such as
  `Unknown(raw)` and test it independently. Do not infer serializer behavior
  from the attribute.
- **Tradeoff:** Closed public domains may intentionally permit exhaustive
  downstream matches. Open domains trade some downstream exhaustiveness for an
  explicit source-evolution promise.
- **Verification:** Compile a downstream consumer fixture or run
  semantic-version checks when changing a public item; preserve meaningful
  internal exhaustive matches; test unknown runtime values with independent
  vectors.

#### R184. Use `#[must_use]` selectively where discard likely indicates a defect

- **Strength:** SHOULD
- **Scope:** value-returning types, functions, methods, and traits
- **Rule:** Add `#[must_use]` when discarding the result usually means the caller
  missed an error, lost a transformation or configuration, or left an operation
  unfinished. Annotate a type when nearly every value requires observation and
  an operation when only that call carries the obligation. Include an actionable
  reason when it improves on the default diagnostic.
- **Noise control:** Do not annotate APIs mechanically or accept every Clippy
  candidate. Avoid redundant function annotations for already must-use return
  types unless a specific message adds useful context. Warning fatigue weakens
  the signal for consequential mistakes.
- **Correctness boundary:** `#[must_use]` is a suppressible lint, not enforcement
  of safety, soundness, transaction completion, resource lifetime, or security.
  APIs must remain safe when values are ignored. Use explicit discard syntax
  such as `let _ = value` or `_ = value` when ignoring a result is intentional,
  while still following R176 for error observability.
- **Trait placement:** Put method policy on the trait declaration. An attribute
  added only to a trait implementation method does not enforce the diagnostic.
- **Compatibility:** Treat adding the attribute as a generally compatible lint
  change that can still fail direct consumers which deny warnings. Consider
  representative consumer builds and release notes for broad changes to
  established APIs.
- **Exceptions:** Cheap queries, ordinary returned data, internal helpers,
  generated code, and side-effecting operations may remain unannotated when
  intentional discard is common or the warning supplies no useful correction.
- **Verification:** Exercise an intentionally ignored call under
  `unused_must_use`; run the curated Clippy policy; inspect downstream consumers
  when broadening an established public diagnostic contract.

#### R185. Name Cargo features for capability or ecosystem truthfully

- **Strength:** SHOULD
- **Scope:** public Cargo features for optional capabilities and ecosystem
  integrations
- **Rule:** Use a capability name such as `async` or `parallel` only when the
  enabled API, required runtime or pool, observable semantics, and compatibility
  promise are genuinely ecosystem-neutral. Use the concrete ecosystem name such
  as `tokio`, `rayon`, or `serde` when callers opt into that ecosystem's types,
  traits, runtime, pool, or semantics.
- **Manifest design:** Keep features positive and additive, avoid empty
  `use-*`/`with-*` wording, and use `dep:dependency` when an optional dependency
  is an implementation detail that should not create an implicit public feature.
  Document what every public feature enables, including dependency and runtime
  consequences.
- **Compatibility:** Feature names and promised behavior are public API. Adding
  one is usually compatible; removing, renaming, or gating existing public
  behavior is normally breaking. Use a documented forwarding alias for staged
  migrations when compatibility policy requires it.
- **Tradeoff:** A capability feature may have a private implementation dependency
  only when consumers do not inherit that dependency's API, runtime ownership,
  or observable semantics. Do not use a broad name merely because an
  implementation might become neutral later.
- **Verification:** Test meaningful features independently plus default,
  no-default, and interaction-prone combinations. Inspect activation with
  `cargo tree -e features`; do not treat an all-features build as sufficient
  evidence.

#### R186. Return async work for the caller to drive by default

- **Strength:** SHOULD
- **Scope:** reusable async libraries, drivers, streams, and runtime-specific
  adapters
- **Rule:** Prefer returning an async result, `Future`, `Stream`, or explicit
  driver future so the caller decides whether and where to await, compose, race,
  or spawn it. Keep the reusable core free of ambient runtime lookup and private
  runtime creation.
- **Runtime adapters:** A Tokio-specific adapter may return work that requires
  Tokio, but must name and document that coupling. Spawn internally only when a
  background lifecycle is intrinsic to the abstraction and returning the work
  cannot express its supervision. Accept an explicit caller-provided
  `tokio::runtime::Handle` or spawner rather than calling `Handle::current` or
  capturing an ambient runtime implicitly.
- **Lifecycle:** Return or retain a task or supervisor handle and document who
  observes completion, output, errors, panics, cancellation, and cleanup. Define
  whether drop aborts, joins, requests shutdown, detaches, or leaves work
  running. Do not discard a join handle unless another documented supervisor
  owns those responsibilities.
- **Blocking work:** Apply the same ownership rule to `spawn_blocking` and local
  executors. Keep sustained DSP compute on the repository's explicit compute
  path rather than executor blocking pools.
- **Exceptions:** An application crate may own a fixed ambient runtime under
  local policy. A framework callback may rely on a guaranteed entered runtime or
  local executor when that requirement and its panic behavior are explicit.
- **Verification:** Construct reusable APIs without ambient runtime context;
  exercise explicit-runtime adapters; deterministically test completion,
  cancellation, shutdown, handle drop, errors, panics, and cleanup.

#### R187. Specify queue semantics before selecting a queue implementation

- **Strength:** MUST
- **Scope:** production queues, channels, mailboxes, admission buffers, and
  reusable-buffer pools
- **Rule:** Define the required behavior before choosing a crate or primitive:
  bounded capacity and its unit, full-queue result, ordering, producer and
  consumer multiplicity, blocking or async wakeup behavior, fairness where it
  matters, cancellation, closure, shutdown and drain behavior, buffer return,
  and observability. Shared Rust guidance defines these semantics but does not
  bless one queue crate for every repository.
- **Repository selection:** During onboarding or a material dependency review,
  evaluate the candidate against the repository's runtime, synchronization
  model, targets, MSRV, dependency and unsafe policy, maintenance requirements,
  and the exact semantics above. Record the selected implementation and any
  semantic mismatch or adapter responsibility locally.
- **Domain boundary:** When drop-oldest, coalescing, recycling, metadata
  attachment, or multi-step shutdown is not one atomic operation of the selected
  primitive, encapsulate it behind a small domain-specific queue type. That type
  owns the race behavior and returns explicit outcomes such as accepted,
  replaced, rejected, closed, or backpressured. Do not scatter ad hoc
  `try_send`, receive, and retry sequences across producers and consumers.
- **Why:** Similar-looking channel APIs make different guarantees, and a
  repository's runtime, target, and dependency constraints legitimately change
  the best implementation. A narrow wrapper can make composite behavior
  consistent without inventing a universal queue framework.
- **Exceptions:** Direct use is preferable when one primitive already expresses
  the complete local contract clearly and the behavior is not duplicated. A
  repository may standardize one implementation locally after review; that
  choice does not become an organization-wide default.
- **Verification:** Test below, at, and above capacity; exact ordering and
  overload outcomes; closure and shutdown; buffer return; discontinuity or
  coalescing metadata; and relevant concurrent races. Do not infer composite
  atomicity, fairness, cancellation safety, or wakeup behavior from an API name.

#### R188. Keep streaming and receiver vocabulary semantically precise

- **Strength:** SHOULD
- **Scope:** SDR, DSP, streaming, receiver, and protocol-facing APIs,
  documentation, tests, metrics, and reviews
- **Rule:** Inspect and follow the repository glossary. Where no repository term
  overrides it, use the reviewed `libsdr` distinctions: a sample is one scalar
  or complex measurement at one instant; an IQ sample is one complex I/Q
  measurement; native IQ combines a source representation with normalization
  metadata; a capture may span blocks and is not necessarily gap-free; a chunk
  is one call-sized piece of a continuous stream; a block is a complete
  contiguous span and ownership-transfer unit whose continuity-sensitive uses
  bind explicit start metadata; and a dwell is an application-selected
  observation interval rather than a DSP buffer.
- **Stage boundaries:** Keep channelization, demodulation, symbol recovery,
  synchronization, detection, and decoding distinct. A selected channel carries
  a rate/passband/guard input contract. Demodulation produces a modulation
  waveform, recovery produces symbol decisions, synchronization matches or
  tracks symbol patterns, detection chooses among supported hypotheses or
  complete receiver paths, and decoding frames, corrects, validates, and
  interprets wire structure.
- **Evidence claims:** Use received for exact pre-correction evidence and
  recovered for post-recovery or post-FEC values, qualifying the layer where
  needed. Do not let configuration fabricate observation evidence. A detector
  reports; retuning, persistence, and policy belong to the caller or an
  explicitly broader controller.
- **Repository mapping:** Define ambiguous sample units, multichannel frames,
  algorithmic blocks, and local type mappings. Repository terminology has
  precedence. Treat a public vocabulary change as a coordinated API migration
  across code, docs, examples, tests, errors, metrics, snapshots, and known
  consumers rather than a mechanical rename.
- **RSL adoption:** RSL repositories using or interoperating with `libsdr`
  adopt its final `IqSample`, `IqBlock`, `Ci16Sample`, receiver/evidence
  vocabulary, title-cased acronym convention, `as_u64` newtype accessors, and
  established `Options`/`Profile`/`Spec`/`Config`/`Policy` suffix meanings unless
  a higher-precedence local or specification-owned term applies.
- **Why:** Adjacent pipeline concepts can share the same underlying Rust types
  while carrying different timing, continuity, evidence, and ownership
  promises. Stable language makes those contracts reviewable.
- **Verification:** Review public vocabulary and source-to-guide consistency;
  test chunking and continuity claims; run semantic-version and known-consumer
  checks for renames.
- **Source:** Owner-approved extraction from the proprietary RSL `libsdr`
  `consolidated-work` branch at
  `15dc4625e1dea2ae64e800a83ade78f24090be36`, reviewed 2026-07-24.

#### R189. Bind metadata at continuity-sensitive stream boundaries

- **Strength:** MUST
- **Scope:** sample acquisition, capture, queues, streaming transports,
  continuity-sensitive stages, and rate- or channel-changing boundaries
- **Rule:** Do not force every data buffer to carry universal metadata. A simple
  finite or offline block may own only contiguous samples when its context is
  explicit. When an API claims stream position, adjacency, sample rate, channel
  geometry, or discontinuity behavior, bind its payload to the metadata required
  for that claim so safe callers cannot accidentally reorder, discard, or
  misassociate them.
- **Metadata selection:** A continuity-aware block normally identifies its
  stream epoch or restart, first sample and index unit, sample rate, and
  discontinuity state. Include block sequence, channel selection, timestamps,
  or diagnostic timing only when the contract needs them. Do not create one
  universal envelope filled with irrelevant optional fields.
- **Kernel boundary:** Let kernels borrow samples plus only the narrow context
  required for computation. The enclosing stream stage retains responsibility
  for metadata propagation and transformation. Do not place authoritative
  payload and metadata in independent queues or separately mutable records.
- **Provenance and transformations:** Metadata is established by the boundary
  that knows it, not invented downstream. Incomplete or discontinuous data must
  not use an accessor that promises a complete continuous block. Rate-changing,
  channel-changing, length-changing, and time-mapping stages construct correct
  output metadata rather than copying stale values.
- **RSL adoption:** In `libsdr`, `IqBlock` remains suitable data-only finite
  storage. Timed sample buffers bind complete sample spans to
  `SampleBlockStart`/`SampleBlockMetadata` and discontinuity state;
  selected-channel buffers bind samples to their channel-selection contract.
- **Why:** Universal metadata bloats simple kernels and invites meaningless
  optionals, while separate parallel metadata can drift from the data it
  describes. Binding only at semantic boundaries preserves both ergonomics and
  correctness.
- **Verification:** Exercise complete, partial, discontinuous, reordered,
  restarted, rate-changed, and channel-changed paths. Use API or compile-fail
  tests where types are intended to prevent separation.

#### R190. Require a complete contract from shared processing traits

- **Strength:** SHOULD
- **Scope:** shared traits and trait objects for DSP, audio, streaming, parsing,
  and receiver stages
- **Rule:** Do not define a universal processing trait. Introduce a shared trait
  only for a demonstrated composition boundary whose real implementations
  perform one coherent operation. The contract defines input representation and
  ownership, output ownership or destination, consumed and produced units,
  output capacity or size bounds, latency, internal buffering, empty-input and
  arbitrary-chunk behavior, reset, discontinuity, flush/end-of-stream, errors,
  and post-error state. Mark a concern inapplicable rather than leaving it
  accidentally undefined.
- **Dispatch:** Use concrete types, generics, and associated types for static
  composition. Prefer an enum for a closed runtime-selected set. Reserve trait
  objects for genuinely runtime-configurable or open heterogeneity such as
  plugins, runtime stage reordering, or application-selected stage types. Keep
  dynamic dispatch outside measured per-sample kernels where practical and
  measure it when relevant.
- **Object safety and adapters:** Decide object safety deliberately. A dynamic
  adapter may normalize richer concrete APIs but must preserve counts, bounds,
  metadata, errors, and lifecycle. Do not allocate, clone buffers, weaken error
  types, or erase discontinuity behavior merely to fit object-safe dispatch.
- **Compatibility:** Public required methods, associated types, supertraits,
  object-safety, implementor freedom, and defaults are API commitments. Seal the
  trait when downstream implementations are not supported; otherwise compile an
  external-style implementor.
- **Why:** A small common denominator can make incompatible stage semantics look
  interchangeable and move correctness obligations into undocumented caller
  convention. Purpose-specific capabilities retain useful composition without
  flattening the domain.
- **Verification:** Run one shared conformance suite against every
  implementation, including one-shot/chunked, reset, flush, discontinuity,
  output-bound, and error-state behavior as applicable. Compare concrete and
  dynamic paths for correctness and measured cost when both exist.

#### R191. Represent exact rate relationships as reduced output/input rationals

- **Strength:** MUST
- **Scope:** decimators, interpolators, rational resamplers, framers,
  clock-domain conversions, and cross-rate metadata
- **Rule:** Represent an exact constant rate relationship as a positive reduced
  rational in the explicit direction `output/input`. Name numerator and
  denominator for that direction, or document the conventional `L/M` relation
  `output_rate = input_rate × L/M`. Preserve absolute input/output rates as
  separate domain values and validate agreement when both forms are supplied.
- **Arithmetic:** Reject zero terms, reduce by the greatest common divisor, and
  use checked integer arithmetic for sizes, capacities, positions, durations,
  and metadata. Do not use floating point to establish exact bounds. Return an
  error for overflow or unrepresentable results rather than wrapping, silently
  saturating, or truncating an allocation.
- **State-aware bounds:** Sizing APIs account for fractional phase, buffered
  input, pending output, startup latency, and flush/tail behavior. Provide
  equivalents of `max_output_for(input_len)` and
  `input_needed_for(output_len)` where callers allocate or schedule work, and
  name whether each result applies to current, reset, steady, or final state.
  Return actual consumed/produced counts when processing does not make them
  otherwise unambiguous.
- **Chunking and mapping:** Carry fractional phase rather than rounding each
  chunk independently. Apply the same relationship, with declared latency and
  reference offsets, when mapping positions and discontinuities.
- **Variable-rate exception:** Adaptive/asynchronous conversion exposes its time
  base, control or estimation state, supported range, and conservative bounds;
  it must not present an estimate as an exact rational.
- **RSL adoption:** `libsdr` documents `L` as interpolation/output numerator and
  `M` as decimation/input denominator in `F_out = F_in × L/M`. Preserve that
  wording. Treat reduction, checked state-aware sizing, and explicit bound APIs
  as requirements to verify rather than assumptions inferred from the type name.
- **Why:** Directionless factors and per-chunk floating-point rounding create
  reciprocal mistakes, capacity errors, and chunk-dependent sample counts.
- **Verification:** Test zero, reduction, identity, near-overflow, floor/ceil
  boundaries, every retained phase, one-shot versus arbitrary partitions,
  flush, discontinuity, and absolute-rate/metadata consistency.

#### R192. Separate reset from finite-stream completion

- **Strength:** MUST
- **Scope:** stateful streaming processors, filters, resamplers, framers,
  encoders, decoders, parsers, and pipelines with buffered or delayed output
- **Reset:** Reset discards buffered input, pending output, fractional phase,
  history, and stream-local progress without emitting it. It restores the
  documented initial state while retaining configuration unless stated
  otherwise. Empty input does not imply reset or end-of-stream.
- **Finish:** `finish`, `close`, or a terminal `flush` declares finite
  end-of-input. It emits every remaining output justified by one explicit tail
  policy or reports the declared incomplete-tail error. The policy states
  whether to drop an incomplete tail, emit a semantically valid partial result,
  pad or synthesize input, or fail.
- **Completed state:** Completion must not duplicate output when repeated. It is
  either idempotent after all output is observed or reports an already-finished
  state. Further input is rejected until reset or explicit reinitialization.
- **Provenance:** Padding, extrapolation, filter warm-down, and other synthetic
  tail material must not be presented as received input. Retain a valid-input
  length, provenance marker, degraded interval, or equivalent evidence when a
  consumer could otherwise confuse them.
- **Live streams and shutdown:** Ordinary chunks in an infinite or live stream
  do not imply completion. At shutdown, the owner explicitly chooses to
  finish/drain the accepted finite prefix or discard/reset pending state. An
  operation that merely exposes currently ready output is named separately from
  finite completion.
- **Why:** Conflating reset, ready-output draining, and end-of-input loses valid
  tail output, leaks state across streams, duplicates output, or fabricates the
  provenance of padded samples.
- **Verification:** Compare reset with a fresh instance and prove it emits
  nothing. Test one-shot and arbitrarily chunked completion, every material tail
  length, repeated completion, input after completion, reset-and-reuse,
  synthetic-tail provenance, and both declared shutdown choices.
- **Acceptable exceptions:** A stateless stage with no buffered or delayed
  output may omit both operations. A stage with no possible tail may make
  completion a documented no-op state transition. Protocol or transactional
  APIs may use domain terms such as `finalize`, `commit`, or `close` while
  preserving the semantic distinction.

#### R193. Name monotonic timing events and export durations

- **Strength:** MUST
- **Scope:** streaming metadata, queue and pipeline instrumentation, latency
  measurement, diagnostics, and APIs exposing monotonic time
- **Events:** Do not attach an ambiguous `timestamp` or `Instant` to every
  buffer. Name the event, such as acquisition start or completion, enqueue
  acceptance, dequeue receipt, processing start or completion, successful
  handoff, or transmission start or completion, and capture time at that event.
  An attempt or pre-await sample is not successful completion.
- **Clock domains:** Use sample positions, rates, epochs, and discontinuities as
  authoritative continuity. Keep hardware/source time, monotonic operational
  time, and wall-clock correlation distinct, stating clock, epoch, and
  uncertainty where applicable.
- **Representation:** Treat `std::time::Instant` and equivalent monotonic
  handles as process-local and opaque. Compare named endpoints from the same
  clock domain, then expose, aggregate, or persist the resulting `Duration`,
  counters, or histograms. Do not serialize an `Instant`, use it as a
  cross-process timestamp, or imply a calendar epoch. Prefer checked duration
  calculation when reversed ordering or clock anomalies are material evidence.
- **Placement and cost:** Keep operational timing in diagnostic sidecars or
  boundary instrumentation unless the consumer's semantic contract needs it.
  Avoid unused per-payload and per-hot-loop timing. Measure timing overhead when
  it could perturb the latency being observed and use bounded sampling or
  aggregation when appropriate.
- **Why:** A generic timestamp hides which latency is measured, pre-send capture
  can mislabel blocking time, and an opaque monotonic handle is not a portable
  event time. Distinct clock domains prevent diagnostics from masquerading as
  sample continuity.
- **Verification:** Use a fake clock or deterministic event source to test
  endpoint ordering and duration labels. Exercise blocked, failed, and
  successful handoffs; reject process-local handles from persisted schemas; and
  measure instrumentation overhead on relevant hot paths.
- **Acceptable exceptions:** Authoritative hardware or protocol timestamps
  remain semantic metadata under their own clock contract. Cross-process
  correlation may use a declared external clock with explicit non-monotonic and
  uncertainty handling. `no_std` code may use a supplied tick source or clock
  trait with equivalent event and scope semantics.

#### R194. Keep library observability typed and caller-routed

- **Strength:** SHOULD
- **Scope:** reusable libraries, applications, services, logging, metrics,
  operational snapshots, structured diagnostics, and telemetry
- **Core contract:** Define operational facts before choosing an ecosystem.
  Expose only the typed, bounded events, snapshots, counters, gauges, or
  histograms that real consumers need. Do not invent a universal observability
  facade or backend trait without demonstrated substitution.
- **Correctness boundary:** Logs and aggregate metrics supplement but never
  replace typed errors, operation results, protocol evidence, per-stream
  discontinuities, or other correctness-bearing state. Emitting a log or metric
  does not handle an error.
- **Logging recommendation:** Prefer `tracing` when a repository chooses
  structured Rust logging and diagnostics, subject to local dependency, MSRV,
  feature, performance, and target policy. A reusable library may instrument
  directly with `tracing` or expose an optional adapter, but it must not install
  a global subscriber or exporter. Applications own `tracing-subscriber`,
  filtering, formatting, export, and process initialization. `tracing` is
  recommended, not required; retain an established `log` or other local
  ecosystem when migration lacks sufficient benefit.
- **Metrics and snapshots:** Do not mandate `metrics`, OpenTelemetry, or another
  exporter across repositories. Let applications adapt typed core evidence.
  Define each instrument's unit, counter monotonicity and
  reset/wrap/saturation behavior, gauge meaning, histogram population and
  buckets, bounded label set and cardinality, aggregation scope, concurrency
  consistency, and export interval where relevant.
- **Cost and data policy:** Avoid unbounded labels and per-item hot-path
  logging. Use filtering, sampling, aggregation, or boundary events; measure
  material instrumentation overhead. Apply repository policy before recording
  secrets, payloads, personal data, or high-volume evidence.
- **Why:** Backend-neutral typed evidence remains testable and reusable, while
  application-owned export avoids global initialization conflicts and forced
  dependency ecosystems. A recommended structured logger gives new repositories
  a good default without erasing legitimate constraints.
- **Verification:** Test typed snapshots and counter semantics, operation
  equivalence with instrumentation disabled or no subscriber installed,
  enabled instrumentation, relevant feature matrices, bounded cardinality,
  application subscriber initialization, sensitive fields, and hot-path cost.
- **Acceptable exceptions:** Applications may standardize a subscriber,
  recorder, or exporter locally. Frameworks may require their facade.
  `no_std`, embedded, FFI, and externally constrained libraries may use compact
  callbacks or snapshots. A second proven backend may justify a narrow adapter
  trait.

#### R195. Keep protocol policy separate from fresh validation evidence

- **Strength:** MUST
- **Scope:** built and parsed protocol messages, validation reports, integrity
  and correction status, trusted wrappers, mutation APIs, and downstream
  operations that require validated input
- **Rule:** Treat construction or parsing policy as input to the operation, not
  as part of message identity. Do not normally store that policy in every built
  value or include it in equality, hashing, or wire encoding. Preserve
  validation evidence separately when callers need to know what was checked and
  observed.
- **Evidence states:** Distinguish passed, failed, skipped or not checked, and
  inapplicable checks wherever those states change caller behavior. Preserve
  protocol-native evidence such as received integrity status, correction
  outcome, unknown or reserved representation, and received versus corrected
  data when the use case needs it. Use a `ValidationReport`, status companion,
  immutable validated form, or domain-specific equivalent rather than treating
  the builder policy as message data.
- **Trust and mutation:** An API that requires trusted input validates at its
  boundary or accepts a validated domain type or wrapper. The validated form
  makes unrestricted safe mutation impossible or exposes only
  invariant-preserving changes. Unrestricted mutation invalidates prior
  evidence and requires revalidation; a stale `validated` flag is a defect.
- **Persistence:** Serialize evidence only under an explicit storage,
  interchange, audit, or protocol contract. Record enough specification and
  validation-version context to interpret persisted results. Do not persist an
  ephemeral construction policy accidentally.
- **Why:** A message's semantic and wire identity should not depend on how a
  caller happened to construct it, while consumers that enforce trust,
  investigate damaged traffic, or measure correction still need precise,
  representation-bound evidence.
- **Verification:** Test equality, hashing, and encoding independently of
  construction policy; passed, failed, skipped, and inapplicable evidence
  states; trusted-input API boundaries; mutation invalidation or compile-time
  prevention; revalidation; received and corrected evidence; and persistence
  compatibility when reports are serialized.
- **Acceptable exceptions:** An immutable message may retain current validation
  status when it is a meaningful domain property. Inspection, forensic,
  safety-critical, or regulated systems may retain both the policy and a full
  report as provenance, provided the record remains distinct from message
  identity and tied to the exact checked representation. A tiny strict-only
  builder may need no report type.

#### R196. Preserve received evidence across protocol correction

- **Strength:** MUST
- **Scope:** decoders with checksums, CRCs, FEC, erasure recovery, damaged-frame
  inspection, channel-quality measurement, and interoperability diagnostics
- **Rule:** When received wire data is retained as evidence, keep the exact
  bytes or symbols immutable and lossless. Produce corrected or recovered data
  separately and keep it associated with the same frame and report. Do not
  overwrite received evidence in place. Use `received`, not `original`, unless
  the transmitter's original representation is independently known.
- **Status model:** Keep scoped integrity observations separate from correction
  outcomes. Represent applicable states such as not checked, passed, and failed
  for the named integrity check, and not attempted, not needed, corrected, and
  uncorrectable for the named recovery operation. Use separate enums, a
  structured report, or an equivalent protocol-specific type. Include
  correction counts, locations, units, confidence, or unknown extent only when
  the implementation can report them truthfully.
- **Meaning boundary:** Successful correction does not make the received
  representation valid, prove recovery of unknowable transmitter-original
  data, or establish semantic validity. An integrity pass likewise does not
  imply that semantic checks passed. Preserve each observation under its
  accurate name and scope.
- **Consumer surfaces:** Inspection and quality-analysis APIs may expose
  received data, recovered data, and the complete report. Ordinary consumers
  may receive the recovered semantic value plus the status needed by their
  trust policy. Treat dropping received evidence as deliberate information loss,
  not as permission to relabel recovered output.
- **Verification:** Use known-answer tests for exact received and recovered
  forms; exercise every applicable integrity and correction state; verify
  correction extents and units; prove received evidence remains unchanged;
  prove correction success does not rewrite integrity history; and prevent
  cross-frame association or stale evidence after mutation.
- **Acceptable exceptions:** A strict decoder that rejects every damaged input
  and has no inspection or quality consumer need not retain received bytes.
  In-place correction is acceptable in a measured constrained path only when
  the API makes that information loss explicit and no promised consumer needs
  received evidence.

#### R197. Make incremental parser outcomes and consumption explicit

- **Strength:** MUST
- **Scope:** stateless parsers, incremental decoders, stream framers, buffered
  protocol readers, resynchronization, and partial-delivery tests
- **Rule:** Distinguish complete, incomplete, and malformed outcomes. Treat
  incomplete input as a normal request for more data rather than malformed input
  or an opaque parse failure. Keep the concrete Rust result type
  repository-specific when these semantics remain explicit.
- **Stateless contract:** A complete result reports its exact consumed prefix
  when trailing data is allowed. An incomplete result consumes nothing; the
  caller retains the full input for retry. If the parser reports how much more
  data it needs, state whether that value is exact or a lower bound and report
  only what the format can determine truthfully.
- **Stateful contract:** A stateful decoder may accept and retain a prefix it
  owns. Distinguish bytes accepted into internal storage from bytes belonging to
  a completed frame, bound retained input, and prevent retry from duplicating or
  losing data.
- **Malformed and recovery:** Keep the failure offset separate from a prefix the
  caller may discard safely. Report a discard or resynchronization count only
  when a protocol-defined marker, length boundary, or other documented
  invariant justifies it. Otherwise let explicit repository policy choose
  recovery.
- **Progress and safety:** Do not spin indefinitely without accepting input,
  requesting more input, consuming a justified prefix, or returning control.
  Continue enforcing non-disableable size, checked-arithmetic, nesting, and
  allocation limits while awaiting completion.
- **Verification:** Split valid frames at every practical byte boundary and
  compare incremental retry with one-shot parsing. Test zero stateless
  consumption on incomplete input, stateful accepted-versus-completed counts,
  retained-buffer limits, malformed offsets, justified resynchronization,
  trailing data, repeated calls, and zero-progress prevention.
- **Acceptable exceptions:** A whole-buffer fixed-size parser may use ordinary
  `Result` when truncation remains directly and unambiguously distinguishable in
  its error type. An adopted parsing framework may use equivalent outcome and
  consumption vocabulary.

#### R198. Give hostile-input parsing finite resource budgets

- **Strength:** MUST
- **Scope:** variable-size frames and messages, recursive or nested formats,
  length- and count-prefixed fields, decoded expansion, incremental buffering,
  allocation, and attacker-influenced parser work
- **Rule:** Define finite limits for every applicable resource dimension,
  including input or frame bytes, field or item counts, nesting or recursion,
  decoded expansion or output bytes, retained incomplete input, and allocation.
  Derive and document repository defaults from protocol maxima and deployment
  needs rather than accidental integer or container limits.
- **Policy boundary:** Keep resource budgets separate from selectable
  protocol-validity checks. Accept caller overrides only through explicit
  validated configuration whose values remain finite. Untrusted fields may be
  compared with a budget but never select, disable, or expand it.
- **Enforcement:** Check declared sizes, counts, cumulative totals, and
  arithmetic before indexing, reserving, allocating, recursing, or performing
  proportional work. Bound per-item and aggregate state where either can grow.
  Return a structured limit-exceeded result instead of waiting for, retaining,
  or allocating an attacker-selected amount.
- **Lifecycle and ownership:** State whether each limit applies per field,
  frame, message, connection, decoder instance, or time window and when it
  resets. Incremental calls cannot evade an aggregate limit. Record defaults,
  units, rationale, hard protocol maxima, deployment overrides, and the owner of
  each choice in repository configuration.
- **Verification:** Test immediately below, at, and above every limit;
  aggregate-versus-per-item growth; checked arithmetic overflow; expansion;
  nesting and recursion; incremental retained input; reset and reconfiguration;
  every validation-policy combination; and attempts by untrusted input to alter
  the active budget.
- **Acceptable exceptions:** Fixed-size, statically bounded formats need no
  runtime policy object when their effective limits are evident and tested.
  Embedded systems may use compile-time capacities. An authoritative protocol
  maximum may be a non-configurable hard limit.

#### R199. Preserve extensible unknown wire values by default

- **Strength:** SHOULD
- **Scope:** extensible protocol discriminants, proxies, inspectors, gateways,
  persisted wire evidence, and forward-compatible decoders
- **Rule:** Preserve unknown non-reserved values losslessly by default when the
  protocol is extensible or consumers proxy, inspect, persist, or round-trip
  them. Keep unknown, reserved, malformed, unsupported, and semantically
  rejected states distinct. Continue rejecting reserved values during strict
  construction unless a named protocol-testing policy permits them.
- **Verification:** Test raw unknown preservation, verbatim re-encoding,
  strict reserved rejection, and every explicitly lossy semantic view.
- **Acceptable exceptions:** A deliberately closed API may reject unknown values
  explicitly. A normalized lossy view may discard them when its inability to
  round-trip is documented.

#### R200. Build protocol corpora from attributed independent evidence

- **Strength:** MUST
- **Scope:** fuzz seeds, known-answer vectors, captured traffic,
  interoperability fixtures, and minimized protocol regressions
- **Rule:** Combine specification-derived vectors, independently implemented
  examples, synthetic boundaries, licensed or internally owned captures, and
  minimized regressions as available. Record origin, revision, license or
  redistribution posture, transformations, expected result, and size limits.
  Keep a short committed smoke corpus separate from sustained or externally
  stored corpora.
- **Verification:** Validate corpus metadata, expected outcomes, fuzz-target
  compilation, smoke execution, and regression promotion.
- **Acceptable exceptions:** A format with an exhaustively testable input space
  may not need a fuzz corpus, but still needs independent conformance evidence.

#### R201. Pin adopted codec implementations in repository policy

- **Strength:** MUST
- **Scope:** adopted protocol libraries, codec frameworks, `bitsandbytes`, and
  reference implementations used as executable evidence
- **Rule:** Record the exact released version or source revision used by each
  repository, its selected features, compatibility expectations, and reviewed
  conventions. The shared skill records the selection method and reference
  lineage rather than imposing one global crate version on every repository.
- **Verification:** Inspect manifests and lockfiles, feature graphs, local
  adoption records, and conformance tests at the pinned version.
- **Acceptable exceptions:** A workspace path dependency may use the exact
  workspace revision and version contract instead of a registry release.

### Scope distinctions and tensions

- **Beginner guidance versus expert directness:** use progressive documentation,
  not duplicated or artificially simplified APIs. Teach the common path while
  keeping precise low-level contracts easy to reach.
- **Comprehensive evidence versus fast feedback:** ordinary test commands should
  remain deterministic and reasonably fast; fuzz, soak, sanitizer, target-
  hardware, and performance suites can run on separate local and CI tiers.
- **Captured realism versus fixture cost:** captured signals and packets improve
  realism but need provenance, redistribution clarity, and deliberate size.
- **Stable vocabulary versus evolving pre-1.0 APIs:** terminology changes are
  allowed, but types, docs, errors, tests, and guides should change together.
- **Cross-platform correctness versus optimization coverage:** every supported
  platform needs a correct path; only the declared first-class matrix promises
  optimization work and routine coverage.

### Repository-specific testing choices deferred to adoption

- Minimum CPU baselines and runtime feature-detection policy within `x86_64` and
  `aarch64`.
- CI access to the LattePanda Sigma, Jetson Nano, Raspberry Pi, and representative
  Mac hardware.
- Per-repository time budgets and exact command mappings for the confirmed test
  tiers.
- Exact fuzzing cadence and external corpus storage backend for each adopting
  repository.
- Which recurring property-testing, model-checking, snapshot, compile-test, and
  fuzz dependencies should be offered through `rsl-deps` after dependency
  review.
- Captured-data storage, size limits, licensing metadata, and regeneration
  conventions.
- Canonical glossary location and vocabulary-change process.
- Placement and format for longer guides outside module rustdoc.
- ADR threshold and storage convention.

## Post-interview synthesis: CodeAesthetic readability

The owner approved extracting general development guidance from the
CodeAesthetic video catalog while explicitly rejecting the blanket conclusion
of `Don't Write Comments`. The channel is advisory. The repository's language,
Rust contracts, measured evidence, and the existing preference record remain
authoritative.

#### R202. Treat shallow control flow as a readability signal, not a law

- **Strength:** SHOULD
- **Scope:** implementation control flow and function extraction
- **Rule:** Use guard clauses, `let ... else`, and concept-level helpers when
  they keep preconditions and the successful path visible. Do not impose a
  numeric indentation or function-length limit, ban every `else`, or fragment a
  cohesive `match` or state transition merely to look flatter.
- **Why:** Visual shape can reduce the number of conditions a reader must retain,
  but a slogan is not a substitute for preserving one understandable decision.
- **Sources:** CodeAesthetic `Why You Shouldn't Nest Your Code`, qualified by
  existing R152-R154 and R159.

#### R203. Prefer domain vocabulary over blanket naming prohibitions

- **Strength:** SHOULD
- **Scope:** identifiers, type names, modules, and public vocabulary
- **Rule:** Name a value or component for its domain role and use unit types or
  unit suffixes when units are otherwise ambiguous. Avoid vague dumping grounds
  such as broad `utils`, `common`, or `helpers` modules. Preserve conventional
  Rust, protocol, mathematical, and repository abbreviations when expanding
  them would make the vocabulary less familiar or precise.
- **Why:** A name should reduce required context. Both opaque shorthand and
  mechanically expanded established terms can make code harder to understand.
- **Sources:** CodeAesthetic `Naming Things in Code`, qualified by R156-R158 and
  the RSL/libsdr vocabulary record.

#### R204. Make useful dependency boundaries explicit without trait inflation

- **Strength:** SHOULD
- **Scope:** components that use replaceable services, policies, resources, or
  algorithms
- **Rule:** Pass dependencies into a component when that separates construction
  policy from use, clarifies ownership, or supports demonstrated configuration,
  substitution, or testing. Choose a concrete type, generic, enum, closure, or
  trait from the real variability and lifetime contract. Do not create an
  interface framework or one trait per dependency solely to enable mocks.
- **Why:** Explicit dependencies can localize configuration and effects, while
  unnecessary interfaces replace hidden coupling with public contract and
  dispatch complexity.
- **Sources:** CodeAesthetic `Dependency Injection, The Best Pattern`, qualified
  by R3, R17, R101, R175, R179, and R181.

#### R205. Charge every abstraction for the coupling it creates

- **Strength:** SHOULD
- **Scope:** shared interfaces, traits, helpers, modules, and generic frameworks
- **Rule:** Add an abstraction when it names a domain concept, contains an
  invariant, removes meaningful duplication, or separates a decision that must
  vary independently from its use. Identify the shared contract and the changes
  it couples. Prefer small duplication when the proposed common boundary joins
  concepts that do not evolve together.
- **Why:** Reuse and substitution can simplify change, but a shared contract also
  constrains inputs, implementors, ownership, and future evolution.
- **Sources:** CodeAesthetic `Abstraction Can Make Your Code Worse` and `The
  Flaws of Inheritance`, translated to Rust composition and qualified by R3,
  R17, R154, R175, and R179-R181.

#### R206. Require workload evidence before trading clarity for speed

- **Strength:** MUST
- **Scope:** performance-motivated review and implementation
- **Rule:** Do not demand a less clear expression, removed helper boundary,
  different collection, allocation trick, or lower-level implementation based
  only on a general speed claim. Identify the shipped workload and requirement,
  profile when applicable, measure a baseline, preserve correctness evidence,
  and measure the proposed change. Prefer material algorithm, data-structure,
  and data-layout improvements before low-impact syntax folklore.
- **Why:** Optimizer behavior and real workloads routinely invalidate
  context-free performance claims.
- **Sources:** CodeAesthetic `Premature Optimization`, qualified by R5,
  R35-R43, R47, R110, R133, R172, and R174.

## Post-interview synthesis: compile-time state modeling

The owner approved codifying the `How to write peak Rust` transformation from
the Let's Get Rusty catalog: replacing repeated runtime guards with types that
cannot express the guarded-against state. The channel remains advisory. The
video presents enum modeling, validated newtypes, and type-state as three
ascending levels of the same idea; this record adopts the first two as defaults
and keeps type-state conditional on a consequential misuse, because a state
parameter charges every caller, signature, and diagnostic for staging that only
pays when the prevented mistake matters. Existing R8-R15, R91, and the builder
guidance remain authoritative on that point.

#### R207. Encode mutually exclusive states in one enum

- **Strength:** SHOULD
- **Scope:** domain and public types whose fields record lifecycle, outcome, or
  mode
- **Rule:** When fields are meaningful only in particular combinations, replace
  them with one enum whose variants own exactly the data each state carries.
  Keep genuinely independent attributes as ordinary fields. Where a flag-and-
  payload representation must survive for a wire, storage, or FFI contract,
  convert to the enum at that boundary and keep the raw form out of domain
  logic.
- **Why:** Parallel flags and optional payloads make the representable state
  space the product of the fields rather than the number of real states, so
  every contradictory combination is constructible and each reader re-derives
  which ones are legal.
- **Sources:** Let's Get Rusty `How to write peak Rust`, qualified by R11, R12,
  R89, R151, and R159.

#### R208. Give a validated type one construction path

- **Strength:** SHOULD
- **Scope:** newtypes and domain wrappers that carry an invariant
- **Rule:** Keep the fields private and make one fallible constructor the only
  way to obtain the type. Do not add a public field, setter, mutable deref, or
  unchecked constructor that re-admits an invalid value. Remove the downstream
  checks the invariant now proves, and state that invariant in the public
  documentation. Name and confine an unchecked constructor when one is genuinely
  required.
- **Why:** The benefit of a validated wrapper is that every later holder may
  skip the check. A second construction path returns the invariant to
  convention, and the deleted checks are no longer there to catch the value that
  slipped through.
- **Sources:** Let's Get Rusty `How to write peak Rust`, qualified by R11, R12,
  R91, and R175.

#### R209. Represent exact quantities exactly and carry their unit

- **Strength:** SHOULD
- **Scope:** monetary amounts, counts, sizes, offsets, indices, and other
  domain quantities whose value must be exact or whose unit is ambiguous
- **Rule:** Represent a quantity that must compare, sum, or round-trip exactly
  as an integer in its smallest meaningful unit or as an exact rational, and
  carry the unit, scale, or currency in the type rather than in a name or a
  comment. Do not use binary floating point for an exact discrete quantity.
  Define rounding, overflow, and mixed-unit behavior on the type using checked
  arithmetic.
- **Why:** Binary floating point cannot represent most decimal fractions, so
  equality, summation order, and round trips drift by amounts small enough to
  pass casual tests, and an unqualified numeric type lets two different units
  mix without complaint.
- **Sources:** Let's Get Rusty `How to write peak Rust`, qualified by R97,
  R102-R106, R156, and R203.

#### R210. Adopt type-state only for a consequential transition

- **Strength:** SHOULD
- **Scope:** resources with a staged lifecycle, such as connection, session,
  authentication, and acquisition handles
- **Rule:** Keep the runtime state field until a misuse is consequential and the
  state graph is small and understandable. When type-state is justified, use
  zero-sized markers to name the states, make the state a type parameter rather
  than a field, bind each operation to the impl block for the state that permits
  it, and make each transition consume the value so a stale handle cannot be
  reused. Remove the runtime guard the staging now proves, and keep a runtime
  check wherever the real state depends on data, peers, or failures the type
  cannot observe.
- **Why:** Compile-time staging removes a class of misuse from the language
  rather than from the test suite, but it also multiplies types, generics,
  diagnostics, and compile cost, and it cannot model a state an external system
  changes underneath the handle.
- **Sources:** Let's Get Rusty `How to write peak Rust`, qualified by R8-R15 and
  R91; the video's presentation of type-state as a universal third level is not
  adopted.

## Round 6: Dependencies, linting, and change discipline

### Confirmed preferences

#### Dependency approval and selection

- Discuss every new dependency with the owner before adding it.
- Prefer `rsl-deps` as the entry point for ordinary dependencies.
- A dependency outside `rsl-deps` requires an explicit additional justification.
- Evaluate maintenance activity, release history, ecosystem adoption, MSRV,
  unsafe usage, security history, license, feature structure, transitive cost,
  and existing alternatives before proposing a crate.
- Check whether the standard library or an existing dependency already solves
  the need.

#### Cargo dependency configuration

- Enable only needed features, while understanding a crate's default features
  before disabling them.
- Centralize shared versions and features through workspace dependencies.
- Investigate duplicate major versions when they materially affect build time,
  binary size, maintenance, or security surface.
- Avoid Git dependencies in released code.
- Pin an exact revision and document a removal plan when a temporary Git
  dependency is approved.
- Exclude optional Tokio, Rayon, SIMD, and comparable integrations from default
  features.

#### MSRV

- Support a moving window of stable Rust releases rather than only the current
  stable toolchain or an indefinitely fixed compiler.
- Use a rolling twelve-month MSRV window by default. Repositories pin an exact
  compiler within that supported window and may declare a justified override.

#### Supply chain and licensing

- Use `cargo-deny` to enforce approved license, advisory, duplicate/version, and
  dependency-source policy.
- Repository-specific dependency and licensing rules take precedence over global
  defaults.
- Reopen dependency discussion when an `rsl-deps` or other dependency change
  expands features or the resolved graph, raises MSRV, adds unsafe exposure, or
  changes behavior. Routine lockfile-only updates within already approved
  constraints do not require a new approval discussion.

#### Formatting and linting

- Use stable rustfmt.
- Inherit lint configuration from the workspace.
- Enable `clippy::all` and a curated subset of `clippy::pedantic`.
- Adopt nursery and restriction lints individually rather than enabling either
  group wholesale.
- Deny warnings in CI with a pinned toolchain.
- Put narrow lint exceptions near the affected code and explain them.
- Avoid repository-wide allowances for isolated issues.
- Give generated code and tests distinct, documented lint treatment where
  justified.

#### Change discipline

- Keep changes scoped to the task and avoid unrelated cleanup or formatting
  churn.
- Document cleanup or improvement opportunities noticed during the task and
  present them to the user as choices rather than silently including them.
- Perform prerequisite refactoring only when it materially reduces implementation
  risk.
- Separate broad refactoring from behavioral changes where practical.
- Update affected tests, documentation, benchmarks, generated files, changelogs,
  and lockfiles according to repository policy.
- Use Conventional Commits for integrated history.
- Distinguish commands actually run from checks not run or merely recommended.

### Draft rules

#### R68. Discuss every new dependency

- **Strength:** MUST
- **Scope:** agent behavior in all repositories
- **Rule:** Obtain owner approval before adding a direct dependency, including one
  available through `rsl-deps`. Present the purpose, alternatives considered,
  relevant costs, and proposed feature configuration.
- **Rationale:** dependencies create durable supply-chain, compatibility,
  maintenance, and API consequences.
- **Acceptable exceptions:** none identified; repository-local instructions may
  impose an even stricter process.

#### R69. Prefer `rsl-deps` for ordinary dependencies

- **Strength:** SHOULD
- **Scope:** repositories participating in the `rsl-deps` dependency model
- **Rule:** Use `rsl-deps` as the entry point for normal approved dependencies.
  When proposing a dependency outside it, explain why `rsl-deps` and existing
  dependencies are insufficient.
- **Rationale:** a common entry point can centralize dependency selection and
  policy across repositories.
- **Acceptable exceptions:** an approved repository-specific or domain-specific
  dependency whose inclusion in `rsl-deps` would be inappropriate.

#### R70. Evaluate dependency fitness before proposing it

- **Strength:** MUST
- **Scope:** new dependency proposals
- **Rule:** Assess maintenance and release activity, adoption, MSRV, unsafe code,
  security record, license, features, transitive graph, replaceability, and
  standard-library or existing-project alternatives.
- **Rationale:** download count or convenience alone does not establish long-term
  fitness.
- **Review questions:** Is the crate still maintained? What unsafe code enters the
  graph? Which default features are enabled? Does its type appear in the public
  API? What would replacement cost?

#### R71. Configure dependency features deliberately

- **Strength:** MUST
- **Scope:** Cargo manifests
- **Rule:** Enable only required features after inspecting default-feature
  behavior. Do not disable defaults mechanically. Keep optional executor,
  parallel, SIMD, and ecosystem integrations out of project default features
  unless the repository explicitly chooses otherwise.
- **Rationale:** features change functionality, transitive cost, portability, and
  security surface.

#### R72. Centralize shared workspace dependencies

- **Strength:** SHOULD
- **Scope:** Cargo workspaces
- **Rule:** Declare shared versions and feature policy in workspace dependencies.
  Investigate duplicate major versions when they materially affect cost or risk.
- **Rationale:** centralization makes version and feature policy visible without
  requiring premature dependency consolidation.

#### R73. Avoid unreleased dependency sources

- **Strength:** SHOULD NOT
- **Scope:** releasable production code
- **Rule:** Do not depend on Git sources in released code. If an exception is
  approved, pin an immutable revision, record why it is needed, and state how it
  will return to a registry release or maintained fork.
- **Rationale:** moving or unpublished sources weaken reproducibility and update
  policy.

#### R74. Declare and test a moving MSRV window

- **Strength:** MUST
- **Scope:** reusable libraries
- **Rule:** State the number of supported stable releases or equivalent time
  window and test its oldest compiler. Treat increasing the lower bound as a
  compatibility change under repository policy.
- **Rationale:** “moving window” is actionable only when its lower bound and
  update cadence are explicit.
- **Mechanical enforcement:** an MSRV CI job plus manifest metadata or repository
  documentation.

#### R75. Enforce repository-aware supply-chain policy

- **Strength:** MUST
- **Scope:** Cargo workspaces
- **Rule:** Use `cargo-deny` or equivalent checks for allowed licenses,
  advisories, sources, and selected duplicate/version constraints. Apply
  repository-specific rules before global defaults.
- **Rationale:** licensing and dependency risk differ by distribution and
  repository context.
- **Mechanical enforcement:** `deny.toml` and CI.

#### R76. Format with stable rustfmt

- **Strength:** MUST
- **Scope:** Rust source
- **Rule:** Use stable rustfmt and repository configuration. Do not introduce
  nightly formatting requirements without an approved repository-specific need.
- **Rationale:** stable formatting reduces toolchain friction and stylistic prompt
  content.
- **Mechanical enforcement:** `cargo fmt --check` in CI.

#### R77. Curate Clippy policy by lint

- **Strength:** MUST
- **Scope:** workspace lint configuration
- **Rule:** Enable `clippy::all`, select pedantic lints that improve the project,
  and adopt nursery or restriction lints individually. Pin the CI toolchain when
  warnings are denied.
- **Rationale:** broad unstable lint groups create churn and include context-
  dependent preferences; explicit selection keeps policy intentional.
- **Mechanical enforcement:** workspace lints plus Clippy CI.

#### R78. Explain lint exceptions narrowly

- **Strength:** MUST
- **Scope:** lint allowances
- **Rule:** Scope an exception to the smallest practical item or generated-code
  boundary and state why the general rule does not apply. Do not add a broad
  allowance for an isolated issue.
- **Rationale:** unexplained allowances silently erode enforcement.

#### R79. Keep task changes scoped

- **Strength:** MUST
- **Scope:** agent-authored changes
- **Rule:** Do not include unrelated cleanup, refactoring, or formatting churn.
  Make prerequisite refactoring only when it materially reduces implementation
  risk, and separate broad refactoring from behavioral changes where practical.
- **Rationale:** scoped changes are easier to understand, test, review, and
  revert.

#### R80. Surface adjacent improvements as choices

- **Strength:** MUST
- **Scope:** agent behavior
- **Rule:** Record cleanup, defects, or improvement opportunities noticed outside
  the task and offer them to the owner as explicit follow-up choices. Do not fix
  them silently.
- **Rationale:** useful observations should not be lost, but noticing an issue
  does not expand task authority.

#### R81. Update affected supporting artifacts

- **Strength:** MUST
- **Scope:** completed changes
- **Rule:** Update affected tests, rustdoc, guides, benchmarks, generated output,
  changelogs, and compatibility notes. Track application lockfiles; follow
  explicit repository policy for library lockfiles.
- **Rationale:** a code-only change can leave the repository inconsistent or its
  consumers uninformed.

#### R82. Report verification truthfully

- **Strength:** MUST
- **Scope:** agent handoff and review summaries
- **Rule:** Separate commands actually run and their results from checks skipped,
  unavailable, or merely recommended. Report material limitations.
- **Rationale:** reviewers must know what evidence exists.

### Scope distinctions and tensions

- **Centralized dependencies versus repository autonomy:** `rsl-deps` is the
  preferred starting point, while repository-local needs may justify exceptions
  after discussion.
- **Minimal features versus default-feature churn:** minimize functionality
  deliberately, not through a blanket `default-features = false` rule.
- **Warnings denied versus compiler evolution:** pin CI toolchains and curate lint
  upgrades so new Clippy releases do not create arbitrary failures.
- **Scoped changes versus valuable observations:** leave unrelated code unchanged,
  but preserve observations by offering them as follow-up choices.
- **Global supply-chain defaults versus legal context:** repository-specific
  license and distribution requirements have higher precedence.

### Unresolved decisions

- Exact versioning, publication, and update process for `rsl-deps`; repository
  research confirmed its role as the zero-default-feature, external-only,
  registry-pinned dependency facade.
- Exact cadence and automation for advancing the confirmed twelve-month MSRV
  window.
- Default global license allowlist and handling of reciprocal licenses such as
  MPL and GPL.
- Advisory exception and vulnerability-response process.
- Duplicate-version thresholds suitable for `cargo-deny`.
- Exact curated pedantic, nursery, and restriction lint sets.
- Whether warnings are denied in local default commands or only CI validation.
- Test-specific lint relaxations and generated-code lint boundaries.
- Changelog thresholds and format.
- Library lockfile defaults where a repository has not stated a policy.

## Round 7: Protocol engineering

### Confirmed preferences

#### Authority and traceability

- Declare the authoritative protocol specification revision, applicable errata,
  and known implementation deviations.
- Treat reference source code as corroborating evidence, not as a silent override
  of the written specification.

#### Trust and parsing

- Treat wire input as hostile by default, including input currently received from
  trusted devices.
- Check lengths, counts, offsets, arithmetic overflow, recursion or nesting
  depth, and allocation limits before indexing or reserving memory.
- Declare finite repository defaults for applicable frame bytes, field counts,
  nesting, decoded expansion, retained incomplete input, and allocation. Keep
  caller overrides explicit and finite; untrusted input cannot alter the active
  budget.
- Separate transport buffering, framing, structural decoding, integrity checks,
  semantic validation, and application interpretation conceptually.
- Simple protocols may combine layers in code, but their responsibilities and
  error locations remain distinguishable.
- Distinguish complete, incomplete, and malformed outcomes. Stateless incomplete
  results consume nothing; stateful decoders report accepted and completed bytes
  separately.
- Keep parse failure locations separate from prefixes callers may safely
  discard.
- Resynchronize only when the protocol provides a reliable marker or boundary;
  choose scan, discard, or connection termination through repository-local
  policy.

#### Unknown and reserved values

- Preserve unknown numeric values when forward compatibility or round-trip
  behavior matters, using a representation such as `Unknown(raw)`.
- Make unknown-value preservation optional when repository semantics do not need
  it.
- Reject reserved values during default construction and permit them through
  explicit validation opt-outs.

#### Validation lifecycle

- Give the builder a strict typed validation policy with all protocol checks
  enabled by default.
- Group relaxable checks by domain meaning and expose named policy methods or
  profiles rather than positional booleans or `validate(false)`.
- Never allow protocol-validity policy to disable memory safety, checked
  arithmetic, internal representation invariants, or finite resource limits.
- Apply the policy during `build()`.
- Treat that policy as operation input rather than message identity; do not
  normally retain it in the built message, equality, hashing, or wire encoding.
- Preserve relevant results as fresh evidence through a validation report,
  protocol status, or validated domain form that distinguishes passed, failed,
  skipped or not checked, and inapplicable checks.
- Do not claim that the resulting owned message remains permanently validated.
- Require trusted-input APIs to validate at the boundary or accept a validated
  form, and invalidate prior evidence after unrestricted mutation.
- Encode the message faithfully without silently restoring disabled validation.
- Provide an explicit `validate()` operation.
- Protect Rust memory and internal representation invariants even when protocol
  validity checks are bypassed.
- Allow repository-local rules to replace or refine this lifecycle through a
  clear escape hatch.

#### Bits, bytes, integrity, and correction

- Document byte order, bit numbering, field width, signedness, padding, and
  reserved-bit behavior beside relevant types and codecs.
- Lean heavily on the owner's `bitsandbytes` crates and their conventions for
  bit- and byte-ordering design.
- Use golden vectors for individual bits, cross-byte boundaries, and complete
  messages.
- Keep integrity validation, error correction, structural parsing, and semantic
  validation conceptually separate.
- When received evidence is retained, keep it exact, immutable, and distinct
  from corrected or recovered output. Do not call received data `original`
  unless the transmitter's original representation is independently known.
- Keep named integrity observations and correction outcomes separate. Expose
  whether correction happened, its truthful extent and units, and received
  versus recovered representations when relevant.
- Do not infer received integrity, transmitter-original equality, or semantic
  validity from successful correction.

### Draft rules

#### R83. Pin protocol authority

- **Strength:** MUST
- **Scope:** protocol implementations
- **Rule:** Record the authoritative specification title, revision, applicable
  errata, and deliberate deviations. Cite exact sections for implemented
  behavior.
- **Rationale:** protocol correctness is relative to a particular normative
  source, not an unnamed general understanding.

#### R84. Keep reference implementations subordinate to specifications

- **Strength:** MUST
- **Scope:** protocol research and implementation
- **Rule:** Use reference code as evidence and an interoperability aid. Do not
  silently follow it when it conflicts with the declared written specification;
  document and resolve the discrepancy.
- **Rationale:** implementations can contain bugs, version drift, and undocumented
  policy.

#### R85. Treat wire input as hostile

- **Strength:** MUST
- **Scope:** framing, parsing, decoding, and validation
- **Rule:** Validate lengths, counts, offsets, arithmetic, nesting, and resource
  limits before indexing, copying, or allocating. Return structured failures
  without panicking. Under R198, declare finite defaults for every applicable
  resource dimension, scope and reset each budget, and prevent untrusted input
  or validation relaxations from changing it.
- **Rationale:** trust in the current sender does not constrain malformed data,
  corruption, future integrations, or adversarial input.

#### R86. Preserve parsing-layer responsibilities

- **Strength:** SHOULD
- **Scope:** protocol implementations
- **Rule:** Keep transport buffering, frame detection, structural decoding,
  integrity checking, semantic validation, and application interpretation
  distinguishable in APIs, types, or internal boundaries.
- **Rationale:** each layer has different failure behavior, state, and test
  evidence.
- **Acceptable exceptions:** simple protocols may combine adjacent layers when
  the combined implementation stays clear and its errors remain attributable.

#### R87. Distinguish incomplete from malformed input

- **Strength:** MUST
- **Scope:** streaming and incremental decoders
- **Rule:** Report that additional bytes are required separately from reporting a
  structurally invalid frame. Under R197, distinguish complete, incomplete, and
  malformed outcomes; consume nothing on stateless incomplete input; and make
  stateful accepted-versus-completed byte ownership explicit. Preserve enough
  state or consumption information for the caller to continue safely, and keep
  failure location separate from an authorized discard prefix.
- **Rationale:** partial delivery is normal for streams and is not a protocol
  error.

#### R88. Make resynchronization policy explicit

- **Strength:** MUST
- **Scope:** streaming decoders
- **Rule:** Resynchronize only using a protocol-defined reliable marker, length
  boundary, or other justified invariant. Let repository-local policy select
  scanning, discarding, or closing when synchronization is lost.
- **Rationale:** heuristic scanning can misidentify payload bytes as frames and
  conceal corruption.

#### R89. Preserve unknown values when semantics require it

- **Strength:** SHOULD
- **Scope:** extensible protocol fields
- **Rule:** Under R199, represent unknown values losslessly, such as
  `Unknown(raw)`, by default when forward compatibility, proxying, inspection,
  persistence, or round-trip fidelity matters. Keep them distinct from reserved,
  malformed, unsupported, and semantically rejected values.
- **Rationale:** rejecting or collapsing unknown values prevents compatible
  evolution and faithful tooling.
- **Acceptable exceptions:** the repository deliberately rejects unknown values,
  or its API explicitly promises a normalized lossy view.

#### R90. Reject reserved values by default

- **Strength:** MUST
- **Scope:** protocol message builders
- **Rule:** Treat reserved values as validation failures under the default policy
  and allow them only through named opt-outs.
- **Rationale:** reserved and unknown are not equivalent: ordinary construction
  should respect the specification while test and research tools retain an
  escape hatch.

#### R91. Make validation policy explicit

- **Strength:** MUST
- **Scope:** protocol construction, parsing, mutation, and intentionally invalid
  message workflows
- **Rule:** Default construction to a strict typed `ValidationPolicy` or
  equivalent named policy owned by the builder. Group selectable checks by
  domain meaning, such as wire conformance and reserved values, integrity,
  canonical representation, and contextual semantics. Use named policy methods
  or profiles rather than `validate(false)`, positional booleans, or an
  unrelated public boolean bag.
- **Safety boundary:** Protocol-validity relaxations never disable memory
  safety, bounds checks, checked length/count/offset arithmetic, internal
  representation invariants, or finite frame, nesting, recursion, and
  allocation limits. Repositories may configure documented finite budgets but
  not remove the boundary through safe input.
- **Independence and trust:** Disabling one validation group does not silently
  disable another. Keep structural parsing, integrity status, semantic validity,
  and application trust distinct. Skipping integrity or authentication must not
  yield a type that falsely claims trusted validation; preserve status or use a
  distinctly unchecked result.
- **Lifecycle:** Apply the selected construction policy during `build()`,
  provide explicit validation after construction, and encode the represented
  message faithfully under R92. Under R195, do not normally retain the policy as
  message identity; retain results separately only when callers need evidence.
  Do not assume a mutable built value remains permanently validated. Use
  separate policy types when hostile-input parsing and construction expose
  materially different choices.
- **Evolution:** Keep policy fields private or otherwise controlled so new checks
  do not force caller struct-literal churn. Document defaults and treat changed
  meanings as behavior changes.
- **Rationale:** a named policy makes deliberate invalid construction readable
  and adaptable without allowing protocol-invalid test cases to compromise
  safety or masquerade as trusted data.
- **Verification:** Test strict rejection for every class, each named relaxation
  in isolation, material combinations, hostile resource inputs under every
  policy, post-build validation, and faithful invalid-message encoding.
- **Acceptable exceptions:** A small protocol may use a few precisely named
  builder methods. An authoritative protocol or adopted library may provide
  equivalent vocabulary. Security-sensitive repositories may make integrity or
  authentication non-relaxable outside dedicated test or inspection APIs.

#### R92. Encode the represented message faithfully

- **Strength:** MUST
- **Scope:** protocol encoding
- **Rule:** Do not silently re-enable validations that the construction path
  explicitly disabled. Encode the represented field values or return a precise
  representational error when the requested wire form cannot be produced.
- **Rationale:** protocol testing requires intentionally invalid messages to reach
  the wire unchanged.

#### R93. Keep memory safety independent of protocol validity

- **Strength:** MUST
- **Scope:** protocol escape hatches
- **Rule:** Validation opt-outs may violate protocol rules but must never permit
  invalid Rust memory, unchecked indexing, arithmetic overflow, impossible
  internal layout, unbounded frame/nesting/allocation growth, or other soundness
  and resource failures through safe code. Apply the non-disableable boundary in
  R91 under every policy.
- **Rationale:** an invalid packet is a supported domain value; memory unsafety is
  not.

#### R94. State bit and byte conventions locally

- **Strength:** MUST
- **Scope:** binary codecs and representations
- **Rule:** Document byte order, bit numbering, field width, signedness, padding,
  and reserved-bit treatment at the relevant type, field group, or codec. Test
  individual bits and cross-byte boundaries with golden vectors.
- **Rationale:** binary-format bugs often arise from conventions that were clear
  only to the original author.

#### R95. Prefer `bitsandbytes` conventions

- **Strength:** SHOULD
- **Scope:** bit- and byte-oriented repositories using the owner's ecosystem
- **Rule:** Begin with the applicable `bitsandbytes` crates and their type and
  ordering conventions. Discuss and justify a different representation before
  introducing it.
- **Rationale:** the crates encode the owner's established approach and can keep
  vocabulary and behavior consistent across projects.
- **Acceptable exceptions:** protocol-specific constraints or measured hot-path
  requirements that the crates cannot meet.

#### R96. Separate integrity, correction, and meaning

- **Strength:** SHOULD
- **Scope:** protocols with CRCs, checksums, or error correction
- **Rule:** Keep structural parse results, received integrity status, correction
  results, and semantic validation distinguishable. Expose whether correction
  occurred, its meaningful extent, and received versus corrected or recovered
  data when the use case needs both. Under R196, keep retained received evidence
  exact and immutable, use `received` rather than `original` when the
  transmitter's value is unknown, and never rewrite integrity history after
  correction. Represent this evidence as a status, report, or validated form
  under R195 rather than retaining an ephemeral builder policy.
- **Rationale:** callers may need to inspect damaged traffic, measure channel
  quality, or distinguish corrected data from originally valid data.

### Scope distinctions and tensions

- **Hostile input versus ergonomic decoding:** resource checks and structured
  failures are mandatory, but should be implemented in lower parsing layers so
  ordinary consumers still receive clear owned values.
- **Specification authority versus real-world interoperability:** reference code
  and observed traffic matter, but deviations from the written standard become
  explicit policy rather than accidental behavior.
- **Unknown preservation versus simpler enums:** preserve raw unknown values where
  evolution and round trips matter; allow deliberately closed repositories to
  reject them.
- **Default validation versus invalid-message tooling:** builders make valid
  construction easiest while named policies permit precise invalid construction
  and faithful encoding.
- **Global validation lifecycle versus repository flexibility:** the proposed
  model is a strong default, not a restriction on protocols whose local
  invariants require a different explicit design.
- **Correction versus evidence preservation:** corrected output is useful, but
  inspection and DSP pipelines may also need the original received form and
  correction metadata.

### Repository-specific decisions

- Exact adopted `bitsandbytes` version or workspace revision and features.
- External sustained-fuzz corpus storage, cadence, and licensed capture sources.

## Round 8: DSP and streaming design

### Confirmed preferences

#### Domain quantities and conversions

- Use explicit domain types for sample rates, frequencies, gains, phases, sample
  counts, timestamps, channel identifiers, and similar concepts.
- Prefer named conversions when they make a semantic distinction visible.
- Do not prohibit `From` implementations; use them when the conversion contract
  is clear and appropriate.

#### Pipeline buffers

- Move an owned domain buffer through pipeline APIs.
- Keep simple finite buffers data-only when their context is explicit. At
  continuity-sensitive capture and transport boundaries, bind the payload to
  the relevant sample rate, channel geometry, timestamp or sample index,
  discontinuity, and related metadata.
- Let processing kernels obtain slices for direct computation.
- Use fixed arrays or const-generic buffers only when size is a genuine compile-
  time invariant.
- Permit plain `Vec<T>` in simple adapters without forcing it through every
  pipeline boundary.

#### Processing composition

- Prefer concrete processor types and statically dispatched generic composition.
- Introduce a common processing trait only when multiple stages genuinely share
  a useful composition contract.

#### Stateful streaming contracts

- Define input consumption, output production, algorithmic latency, internal
  buffering, arbitrary chunk-boundary behavior, reset behavior, flush behavior,
  empty-input behavior, and chunking equivalence for stateful stages.
- Keep reset distinct from finite completion. Reset emits nothing and discards
  stream-local state. Finishing applies an explicit tail policy exactly once;
  live streams finish only when their owner deliberately ends a finite prefix.
- Keep padding and other synthetic tail material distinguishable from received
  input.

#### Rate-changing stages

- Expose output-size bounds or required capacity before processing for
  decimators, interpolators, resamplers, framers, and similar stages.
- Represent rate relationships explicitly.
- Do not allow surprising steady-state buffer growth in hot paths.

#### Discontinuities and timing

- Mark sample loss or discontinuities explicitly and carry a lost count or sample
  index range when known.
- Represent a within-epoch gap with the next absolute index and a known half-open
  loss range or explicit unknown extent. Represent restart, retune, or rate
  change with a new stream epoch rather than a fabricated cross-epoch range.
- Keep the reason for discontinuity separate from evidence of its exact extent.
- Require stateful stages to declare whether they reset, continue with degraded
  output, or return an error after discontinuity.
- Name every measured timing event and capture it at that event. Keep monotonic
  `Instant` values process-local and export or persist derived durations rather
  than ambiguous raw timestamps.
- Treat timing deltas as diagnostic corroboration, not authoritative proof of
  sample loss.

#### Hot-path observability

- Do not log inside per-sample or tight per-block loops.
- Collect cheap measurements and report them at pipeline boundaries.
- Observe dropped samples, queue saturation, processing duration, high-water
  marks, buffer starvation, and allocation fallback where relevant.
- Prefer `tracing` for structured logging and diagnostics when a repository
  chooses that dependency, but do not require it universally or let a reusable
  library install the process subscriber.
- Keep reusable operational evidence typed and bounded, and let applications
  adapt it to their chosen logging, metrics, or telemetry backend.

### Draft rules

#### R97. Represent DSP quantities with domain types

- **Strength:** SHOULD
- **Scope:** DSP libraries and domain-oriented application components
- **Rule:** Use distinct types for quantities whose units, reference frame,
  allowed range, or interpretation matter, including rates, frequencies, gains,
  phases, counts, timestamps, and channel identity.
- **Rationale:** domain types make equations and APIs readable and prevent unit or
  representation confusion.
- **Acceptable exceptions:** a local primitive is unambiguous and a wrapper adds
  no useful invariant or vocabulary.

#### R98. Match conversion traits to conversion semantics

- **Strength:** SHOULD
- **Scope:** domain-type conversions
- **Rule:** Implement `From<T>` only when the conversion is infallible,
  semantically lossless, value-preserving, and the single obvious conversion
  between the types. Do not panic, silently discard meaningful information,
  reinterpret the conceptual value, or conceal a choice in `From`.
- **Alternatives:** Use `TryFrom` for validation or any other possible failure.
  Use a named method or constructor when representation, reference frame, byte
  order, rounding, normalization, policy, or domain interpretation should remain
  visible at the call site.
- **Trait direction:** Implement `From` rather than `Into` directly so the
  standard blanket implementation provides `Into`. Direct `Into`
  implementations are legacy compatibility for toolchains predating the relaxed
  orphan rules and are not needed under the current MSRV.
- **Rationale:** standard traits improve ergonomics, while named operations keep
  consequential semantics readable.
- **Verification:** Test recovery or invariant preservation when it is part of
  the semantic-loss claim, and test every failure class for `TryFrom`.
- **Acceptable exceptions:** repository vocabulary may establish one
  unambiguous conventional conversion suitable for `From`. Incidental
  representation details that are not semantically meaningful, such as spare
  container capacity, need not be preserved.

#### R99. Move owned domain buffers through pipelines

- **Strength:** PREFER
- **Scope:** DSP and streaming pipeline boundaries
- **Rule:** Transfer an owned domain buffer that can retain storage. Bind it to
  relevant stream metadata at continuity-sensitive boundaries, but allow a
  simple finite buffer to remain data-only when rate, position, and geometry are
  explicit in its call context. Give kernels efficient slice access and provide
  plain-`Vec` adapters for simple consumers.
- **Rationale:** ownership transfer supports buffer reuse without globally shared
  mutation, while boundary-specific wrappers preserve continuity and timing
  context without burdening every buffer.

#### R100. Use fixed-size types only for real invariants

- **Strength:** SHOULD
- **Scope:** sample and frame buffers
- **Rule:** Use arrays or const-generic sizes when the algorithm or protocol
  genuinely requires a compile-time size. Do not spread const-generic complexity
  merely to avoid dynamic storage.
- **Rationale:** static size can encode useful invariants, but arbitrary block
  sizes and streaming boundaries often remain runtime concerns.

#### R101. Prefer concrete and static stage composition

- **Strength:** PREFER
- **Scope:** DSP processors and pipelines
- **Rule:** Compose concrete processor types or statically dispatched generics.
  Define a shared trait only when multiple real implementations perform one
  coherent operation at a demonstrated composition boundary. Do not place
  unrelated in-place, rate-changing, framing, and sink stages behind one
  universal processor interface merely because their method signatures can be
  normalized.
- **Rationale:** uniform traits should serve actual composition rather than erase
  meaningful differences among DSP operations.

#### R102. Document stateful streaming behavior

- **Strength:** MUST
- **Scope:** stateful streaming stages
- **Rule:** Define consumed and produced quantities, latency, buffering, chunk-
  boundary behavior, empty input, reset, flush, and end-of-stream behavior.
  Apply R192: reset and finite completion are not synonyms, and empty input is
  neither unless the API explicitly and unambiguously establishes that domain
  convention.
- **Rationale:** hidden streaming state makes otherwise correct kernels fail when
  integrated with arbitrary chunking or shutdown.

#### R103. Test chunking equivalence

- **Strength:** MUST
- **Scope:** streaming stages that promise chunk-independent behavior
- **Rule:** Compare one-shot and variably chunked processing of the same logical
  input under the stage's numerical and latency contract.
- **Rationale:** consumers should not receive different signal meaning solely
  because transport chunk sizes changed.
- **Acceptable exceptions:** the stage explicitly defines block-sensitive
  semantics; document and test those semantics instead.

#### R104. Expose rate-change output bounds

- **Strength:** MUST
- **Scope:** rate-changing and framing stages
- **Rule:** Provide a way to determine required capacity or a safe output bound
  before processing. Represent an exact constant relationship as a reduced
  rational in the named `output/input` direction and keep absolute rates
  separate. Make bounds account for current fractional phase, buffered state,
  startup latency, and flush behavior; distinguish current-state, reset-state,
  steady-state, and final bounds. Avoid unannounced steady-state growth.
- **Rationale:** callers need to size and recycle buffers without speculative
  allocation.

#### R105. Propagate discontinuities explicitly

- **Strength:** MUST
- **Scope:** lossy streaming pipelines
- **Rule:** Bind a discontinuity to the next delivered buffer. Represent a
  within-epoch gap with the stream epoch, the absolute index of the next
  delivered sample, and a loss extent that is either one known half-open range
  `[start, end)` or explicitly unknown. Represent a restart, retune, sample-rate
  change, or equivalent reconfiguration with a new epoch and its first delivered
  index rather than fabricating a cross-epoch loss range.
- **Evidence and reason:** Keep the extent separate from the cause. A device
  overrun, queue overflow, source restart, counter gap, or unknown reason does
  not prove exact lost indices. Derive a known count from the range with checked
  arithmetic rather than storing inconsistent evidence. A zero-length range is
  not loss.
- **Units and accumulation:** Encode or name the index unit, origin, and rollover
  behavior. Coalesce repeated losses before the next delivery only when evidence
  proves one exact within-epoch union. Otherwise retain multiple ranges when
  supported and useful, or report an unknown aggregate extent; do not guess.
- **State policy:** Require each stateful consumer to define reset or
  reinitialization, error, or domain-justified gap-aware continuation. Reset
  before processing the next samples by default; never silently bridge a gap
  with stale filter, synchronization, decoder, or timing state.
- **Propagation:** Forward or transform discontinuity metadata through every
  stage. Rate-changing stages define the input-to-output index mapping, and
  stages with warm-up or reacquisition mark affected output invalid or degraded
  until their normal contract resumes.
- **Observability:** Aggregate drop and reset metrics supplement but do not
  replace the discontinuity carried with stream data. Wall-clock timing remains
  diagnostic rather than proof of an exact sample range.
- **Rationale:** silently bridging a gap can corrupt filter state, timing,
  demodulation, and downstream interpretation.
- **Verification:** Inject known, unknown, repeated, and zero-length loss
  extents before and across buffers; test checked range/count arithmetic;
  distinguish new-epoch restarts from within-epoch gaps; compare reset behavior
  with fresh state; and exercise rate-changing mappings and reacquisition
  boundaries.

#### R106. Keep timing evidence separate from sample continuity

- **Strength:** SHOULD
- **Scope:** streaming metadata and diagnostics
- **Rule:** Instrument timing only at a named event boundary and apply R193.
  Use derived monotonic intervals to detect suspicious delay, but do not infer
  exact sample loss from operational or wall-clock timing alone.
- **Rationale:** scheduling and queueing jitter can change send intervals without
  changing the sample sequence.

#### R107. Keep logging out of DSP hot loops

- **Strength:** MUST
- **Scope:** per-sample and tight per-block processing
- **Rule:** Do not emit logs or traces directly from hot-loop iterations. Collect
  bounded, cheap state and publish it outside the loop or at pipeline boundaries.
- **Rationale:** logging introduces latency, allocation, synchronization, and
  volume hazards that can dominate DSP work.
- **Acceptable exceptions:** temporary, explicitly enabled diagnostic builds that
  are not used for performance claims.

#### R108. Instrument overload and resource behavior

- **Strength:** SHOULD
- **Scope:** streaming applications and pipeline adapters
- **Rule:** Make relevant dropped-sample counts, discontinuities, queue
  saturation, high-water marks, processing duration, buffer starvation, and
  allocation fallback observable at pipeline boundaries.
- **Rationale:** overload and performance policy cannot be validated without
  operational evidence.

### Scope distinctions and tensions

- **Domain buffers versus simple interoperability:** owned domain buffers carry
  useful metadata and storage reuse, while slice and `Vec` adapters keep simple
  consumers direct.
- **Named conversions versus standard traits:** use `From` when meaning is
  obvious; use named methods when an expert or beginner should see a unit,
  reference, normalization, or rounding choice.
- **Arbitrary chunks versus algorithmic blocks:** transport chunk sizes should
  not leak into signal meaning unless the algorithm is explicitly block-sensitive.
- **Discontinuity metadata versus wall-clock timing:** sample indices and explicit
  loss markers establish continuity; monotonic timing helps diagnose but cannot
  prove loss by itself.
- **Observability versus hot-path cost:** collect bounded cheap measurements in
  the hot path and emit or aggregate them outside it.

## Round 9: Agent behavior, precedence, and adoption

### Confirmed preferences

#### Parallel DSP

- For optional Rayon support, let the caller control the pool and concurrency
  level.
- Require a measured minimum grain size before parallelizing work.
- Avoid uncontrolled nested parallelism.
- Preserve required ordering and satisfy the scalar numerical contract.

#### Inspection and planning

- Before editing, inspect applicable instruction files, surrounding code,
  existing patterns, manifests, tests, specifications, and repository commands.
- Inspect recent history when an unusual design lacks a clear current rationale.
- State consequential assumptions and avoid speculative rewrites.
- Continue conservatively when a decision is confined and reversible.
- Ask before broad, persistent, or difficult-to-reverse choices.
- Present alternatives when tradeoffs materially affect architecture.
- Do not interrupt for minor details that repository evidence can answer.

#### Verification and self-review

- Run formatting, applicable Clippy checks, relevant tests, and rustdoc checks by
  default.
- Add feature combinations, fuzzing, Miri, sanitizers, benchmarks, and target-
  specific checks according to the affected risk.
- Review the final diff for public API growth, ownership mistakes, panic paths,
  hot-path allocation, missing documentation, and unrelated changes.

#### Precedence

Apply instructions in this order:

1. Current explicit user instructions.
2. The closest repository-local instruction file.
3. Parent and root repository instructions.
4. Repository-declared domain skills.
5. General Rust skills.
6. General agent behavior.

A lower-precedence layer may strengthen an unconstrained rule but may not
silently override a higher-precedence layer. Report material conflicts.

#### Canonical content and adapters

- Keep canonical standards content tool-neutral.
- Generate thin adapters for Codex, Claude Code, Cursor, Zed, and other supported
  systems.
- Prefer generated adapters over symlinks because discovery and symlink behavior
  vary.
- Mark adapters with their canonical source and version and do not edit them
  directly.
- Author canonical skills as directly readable, Markdown-first Agent Skills with
  structured Markdown reference metadata.
- Generate only thin product adapters initially. Add a richer rule compiler only
  if evals demonstrate a material composition or drift problem.

#### Repository adoption record

Each adopting repository declares:

- applicable engineering profile and domain skills;
- local architecture and dependency boundaries;
- build, lint, test, fuzz, benchmark, and profiling commands;
- performance budgets and designated hot paths;
- supported targets and MSRV;
- trust boundaries and protocol specifications;
- queue overload and shutdown policies;
- unsafe and FFI locations;
- local exceptions and rationale; and
- pinned standards version.

#### Owner-specific source material

- Inspect `rsl-deps` and the `bitsandbytes` crates under the `rsl` repository in
  the `rawsocketlabs` GitHub organization.
- Use their actual APIs, documentation, licenses, and conventions rather than
  inferring policy from their names.

### Draft rules

#### R109. Let callers own parallel execution

- **Strength:** MUST
- **Scope:** optional parallel DSP integrations
- **Rule:** Retain a sequential entry point. Under a `rayon` feature, make the
  parallel entry point accept a caller-owned `&rayon::ThreadPool` and run its
  parallel iterators, joins, or scopes inside `ThreadPool::install`. Do not
  initialize, configure, or silently rely on Rayon's global pool from reusable
  library code.
- **Abstraction:** The concrete pool is truthful for a Rayon-specific feature.
  Introduce an executor trait only after a demonstrated second backend or
  repository boundary requires substitution.
- **Nesting:** Analyze calls from existing Rayon pools because installing into a
  different pool may yield and interleave other work on the waiting pool. Avoid
  uncontrolled nesting, recursive parallelization, and oversubscription.
- **Rationale:** applications need to coordinate CPU budgets across DSP and other
  workloads.
- **Acceptable exceptions:** an application crate may own and configure the
  global pool under explicit local policy.

#### R110. Measure parallelization granularity

- **Strength:** MUST
- **Scope:** parallel DSP implementations
- **Rule:** Select the parallel path only above a workload-specific threshold
  established by size-sweep benchmarks across representative targets, pool
  widths, and production features. Name the threshold's unit and record the
  evidence and hardware assumptions. Do not define one universal grain-size
  constant.
- **Correctness:** Preserve required output ordering and the scalar numerical
  contract through one shared conformance suite. Retain a way for benchmarks to
  force both paths without making forced parallelism the ordinary default.
- **Configuration:** Expose a threshold override only when real consumers need
  materially different cutoffs; otherwise keep a named, documented internal
  decision that can evolve with evidence.
- **Rationale:** parallelism can reduce performance and reproducibility for small
  blocks or poorly partitioned state.

#### R111. Inspect repository context before editing

- **Strength:** MUST
- **Scope:** agent behavior
- **Rule:** Read applicable instructions, relevant surrounding code, existing
  patterns, manifests, tests, specifications, and documented commands before
  editing. Inspect history when the reason for an unusual durable design is
  unclear.
- **Rationale:** repository-specific facts and prior decisions outrank generic
  preferences.

#### R112. Make uncertainty proportional to blast radius

- **Strength:** MUST
- **Scope:** agent behavior
- **Rule:** State consequential assumptions, continue with confined reversible
  choices, and ask before broad or difficult-to-reverse decisions. Present
  materially different architectural alternatives without interrupting for
  details repository evidence can resolve.
- **Rationale:** this preserves momentum without silently committing the owner to
  expensive architecture.

#### R113. Verify according to affected risk

- **Strength:** MUST
- **Scope:** agent-authored changes
- **Rule:** Run formatting, applicable Clippy, relevant tests, and rustdoc checks
  by default. Add feature, fuzz, Miri, sanitizer, benchmark, performance, and
  target checks when the changed risk requires them.
- **Rationale:** verification should be consistent but not an undifferentiated
  maximal command set for every edit.

#### R114. Review the completed diff

- **Strength:** MUST
- **Scope:** agent behavior before handoff
- **Rule:** Inspect the diff for correctness, unnecessary public API growth,
  ownership and allocation mistakes, panic paths, error quality, unsafe
  invariants, missing tests or documentation, unsupported performance claims,
  and unrelated changes.
- **Rationale:** command success does not establish architectural quality or task
  scope.

#### R115. Apply explicit instruction precedence

- **Strength:** MUST
- **Scope:** all agent work
- **Rule:** Apply current user instructions, closest local instructions, ancestor
  repository instructions, declared domain skills, general Rust skills, and
  general behavior in descending order. Do not let a lower layer silently
  override a higher one; report material conflicts.
- **Rationale:** predictable conflict resolution is necessary when standards and
  repositories evolve independently.

#### R116. Generate thin tool adapters

- **Strength:** SHOULD
- **Scope:** standards distribution
- **Rule:** Maintain tool-neutral canonical content and generate the smallest
  adapter required for each supported agent. Mark generated files with source
  version and edit instructions.
- **Rationale:** one source prevents Codex, Claude, Cursor, and Zed guidance from
  drifting while respecting different discovery mechanisms.
- **Acceptable exceptions:** genuinely tool-specific behavior belongs in a small
  tool-specific source template rather than the canonical Rust rules.

#### R117. Prefer generated adapters over symlinks

- **Strength:** PREFER
- **Scope:** cross-agent distribution
- **Rule:** Materialize generated adapter files unless verified tool behavior and
  target platforms make symlinks reliable.
- **Rationale:** symlink handling differs across tools, operating systems,
  archives, and installation methods.

#### R118. Declare repository adoption context

- **Strength:** MUST
- **Scope:** repositories adopting the standards
- **Rule:** Pin the standards version and declare profile, applicable skills,
  architecture, dependency boundaries, commands, performance constraints,
  targets, MSRV, trust boundaries, protocol sources, overload and shutdown
  policy, unsafe/FFI locations, and local exceptions.
- **Rationale:** global skills cannot infer facts and policies unique to a
  repository.

### Scope distinctions and tensions

- **Thorough inspection versus momentum:** inspect evidence relevant to the
  change, but do not turn minor work into an exhaustive repository archaeology
  exercise.
- **Default verification versus risk-based expansion:** formatting, linting,
  tests, and docs are baseline evidence; expensive tools activate when the
  affected risk warrants them.
- **Local precedence versus global consistency:** repository instructions may
  override general preferences, but exceptions become explicit and reviewable.
- **Canonical neutrality versus tool discovery:** engineering judgment stays
  tool-neutral while small generated adapters express actual trigger and loading
  behavior.
- **Parallel throughput versus application CPU ownership:** Rayon can accelerate
  suitable workloads, but the library does not seize an application-wide pool or
  hide its scheduling policy.

### Unresolved decisions

- Exact schema and filename for repository adoption declarations.
- How nested repository instructions identify and justify overrides.
- Versioning and regression checks for the now-researched discovery, nesting,
  and precedence behavior of each supported agent.
- Adapter version header and drift-detection mechanism.
- Local and global installation paths, pinning format, and update workflow.
- Verification command profiles and how repository-local commands override them.
- How generated standards adapters track changes to the researched
  `RawSocketLabs/rsl/rsl-deps` and `RawSocketLabs/rsl/bitsandbytes` conventions.

## Stage 1 confirmations, architecture approval, and deferred refinement

### Confirmed standards-system decisions

- License the standards and skills system under the dual MIT OR Apache-2.0
  model, matching Rust and the reviewed RSL repository.
- Use the twelve-month moving MSRV window recorded above.
- Treat Apple Silicon as the first-class macOS target and Intel macOS as a
  correctness-oriented, non-optimized target unless a repository says otherwise.
- Apply the material dependency-change approval threshold recorded above.
- Begin with Markdown-first Agent Skills and generated thin adapters rather than
  a general rule compiler.
- Proceed with the proposed two-skill architecture through Stage 2A preference
  refinement. This approval does not yet authorize skill or tooling
  implementation.
- Host the canonical standards component at `tools/rust-skills` beneath RSL
  while preserving independently versioned release tags and exports.

### Draft rules

#### R119. License the standards system explicitly

- **Strength:** MUST
- **Scope:** the reusable standards and skills component
- **Rule:** Publish the system under MIT OR Apache-2.0 and include both license
  texts before distributing generated skills.
- **Why:** The system needs an unambiguous reuse grant, and the selected model
  aligns with the Rust ecosystem and RSL owner code.

#### R120. Maintain a twelve-month MSRV window

- **Strength:** SHOULD
- **Scope:** repositories without a stricter local policy
- **Rule:** Keep the default repository MSRV no more than twelve months behind
  the current stable Rust release, pin it exactly, and move the pin through a
  reviewed change. Test the pinned MSRV and the repository's current stable or
  pinned development toolchain.
- **Exception:** A repository MAY pin outside the window when hardware, vendor,
  ecosystem, or deployment constraints are documented locally.

#### R121. Tier macOS architecture support

- **Strength:** SHOULD
- **Scope:** repositories adopting the default platform matrix
- **Rule:** Test Apple Silicon macOS as first class. Preserve correct Intel macOS
  behavior when practical, but require Intel-specific optimization only when a
  repository declares it.

#### R122. Reapprove material dependency changes

- **Strength:** MUST
- **Scope:** direct dependencies and `rsl-deps` capabilities
- **Rule:** Discuss a dependency change when it expands features or the resolved
  graph, raises MSRV, changes unsafe exposure, or changes behavior. A lockfile-
  only update inside previously approved constraints MAY proceed through the
  repository's normal update process.

#### R123. Keep canonical authoring Markdown-first

- **Strength:** MUST
- **Scope:** the initial standards-system implementation
- **Rule:** Keep canonical skill packages directly readable and independently
  reviewable. Generate target adapters, but do not introduce a general rule
  compiler until comparative evals show that the added abstraction solves a
  material problem.
- **Why:** This preserves clarity, keeps failures inspectable, and leaves a path
  to richer tooling if duplication or composition becomes costly.

#### R124. Keep the standards bundle relocatable

- **Strength:** MUST
- **Scope:** standards-system layout, tooling, distribution, and adoption
- **Rule:** Keep the standards component independently versioned and operable
  from its canonical `tools/rust-skills` location or as a standalone export.
  Resolve resources relative to the standards source, avoid hardcoded parent
  paths, do not join RSL's root Cargo workspace, and require explicit adapter
  installation rather than activation merely because the source is present.
- **Why:** Canonical hosting belongs in the broader RSL source organization,
  while the skills retain a separate release contract, build graph, and
  discovery lifecycle.
- **Mechanical enforcement:** Relocation tests exercise generation and
  validation in standalone and nested fixture layouts; generated manifests
  record the semantic standards version and source hashes.

### Stage 2A refinement outcome

The owner completed dedicated refinement of:

- testing standards beyond the current portfolio, including test structure,
  evidence quality, naming, coverage, fixtures, and command tiers;
- examples, distinguishing skill examples, rustdoc examples, repository
  `examples/`, sample applications, and example maintenance requirements; and
- code style preferences that are not fully expressed by rustfmt or Clippy,
  including the preference for `match` over some `if` forms.

These decisions now inform `rsl-rust-core`, `rsl-rust-review`, repository
templates, and evals. They do not by themselves authorize implementation.

The canonical source is a directly tracked directory at `tools/rust-skills` in
`RawSocketLabs/rsl`, not a submodule. External consumers may pin a namespaced RSL
tag, an exact commit, or a release archive. Every delivery form must preserve
the constraints in R124.

## Revision history

- 2026-07-18: Created from interview Round 1.
- 2026-07-18: Selected `RawSocketLabs/rsl/tools/rust-skills` as the canonical,
  directly tracked source location with separate namespaced releases.
- 2026-07-18: Added API, ownership, dispatch, error, and panic preferences from
  interview Round 2.
- 2026-07-18: Added execution, channel, overload, buffer-recycling, shared-state,
  and lifecycle preferences from interview Round 3.
- 2026-07-18: Confirmed transport overload defaults and added performance,
  numerical, unsafe, and FFI preferences from interview Round 4.
- 2026-07-18: Added platform, testing, fixture, documentation, vocabulary, and
  guide preferences from interview Round 5.
- 2026-07-18: Added dependency approval, `rsl-deps`, MSRV, supply-chain, lint,
  formatting, and change-discipline preferences from interview Round 6.
- 2026-07-18: Added protocol authority, hostile-input, parser-layering, validation,
  binary representation, `bitsandbytes`, and correction preferences from
  interview Round 7.
- 2026-07-18: Added DSP domain-type, pipeline-buffer, streaming-contract,
  discontinuity, timing, and observability preferences from interview Round 8.
- 2026-07-18: Added parallel execution, agent inspection, verification,
  precedence, adapter, and repository-adoption preferences from interview Round
  9.
- 2026-07-24: Added borrowed-optional API and frozen-sequence ownership
  decisions after reviewing owner preferences, official standard-library
  documentation, Clippy's `ref_option` guidance, and Logan Smith's advisory
  videos.
- 2026-07-24: Confirmed a narrow runtime-heterogeneity exception for DSP and
  audio pipeline trait objects, and added measured, profile-specific binary-size
  guidance informed by Cargo documentation and `min-sized-rust`.
- 2026-07-24: Confirmed “parse, don't validate” as an organization-wide
  preference for durable trust-boundary invariants, with explicit raw,
  transient, protocol-testing, and context-dependent exceptions.
- 2026-07-24: Made representative per-sample benchmark state mandatory and
  classified invalid state setup as blocking when it supports a performance
  claim.
- 2026-07-24: Rejected common capability traits as a checklist; required
  meaningful clone/default/serialization semantics and truthful `Send`/`Sync`
  properties.
- 2026-07-24: Prohibited silent `Result`-to-absence conversion unless error loss
  is an explicit, documented, observable, and tested policy.
- 2026-07-24: Preferred the narrowest meaningful borrow capability and approved
  field-level helpers when whole-object receivers create concrete coupling,
  while preserving multi-field invariants, compatibility, and clarity.
- 2026-07-24: Required queue semantics to be specified independently of a
  concrete crate, selected implementations to pass repository dependency review,
  and nontrivial composite behavior to live behind a small domain queue type.
- 2026-07-24: Made production work owned and joined by default and required a
  per-work-class lifecycle record covering admission, shutdown, drain or discard,
  buffer return, bounded join, timeout escalation, results, panics, and explicit
  detachment exceptions.
- 2026-07-24: Adopted the reviewed `libsdr` sample/chunk/block/dwell, receiver
  stage, and evidence distinctions as portable defaults while preserving exact
  `libsdr` type spellings and naming conventions in the RSL organization layer.
- 2026-07-24: Kept simple finite sample buffers data-only while requiring
  continuity-sensitive capture and transport boundaries to bind payloads to the
  metadata needed for their stream, rate, channel, and discontinuity claims.
- 2026-07-24: Rejected a universal processing trait; retained concrete/static
  composition by default and required any shared stage trait to define a
  coherent, complete streaming contract, with trait objects reserved for real
  runtime heterogeneity.
- 2026-07-24: Required exact rate relationships to use a reduced, directed
  `output/input` rational with checked state-aware capacity bounds, retained
  fractional phase, explicit flush scope, and a separate variable-rate contract.
- 2026-07-24: Separated reset from finite-stream completion, required an explicit
  tail policy and nonduplicating completed state, and kept synthetic tail data
  distinguishable from received input.
- 2026-07-24: Replaced ambiguous per-buffer timestamps with named timing events,
  process-local monotonic handles, derived duration exports, and explicit
  separation from source clocks and stream continuity.
- 2026-07-24: Standardized discontinuity evidence as a typed within-epoch gap
  with next absolute index and known half-open or unknown loss extent, while
  representing restart and reconfiguration with a new stream epoch.
- 2026-07-24: Kept reusable observability typed, bounded, and caller-routed;
  recommended but did not require `tracing` for structured logging; left
  subscriber, metrics, and telemetry backend selection to applications.
- 2026-07-24: Made protocol validation policies strict and typed by default,
  grouped relaxations by domain meaning, and prohibited every policy from
  disabling safety, checked arithmetic, internal invariants, or finite resource
  limits.
- 2026-07-24: Kept construction and parsing policy outside normal message
  identity, preserved relevant validation and correction evidence separately,
  and required trusted wrappers or boundary validation whose evidence cannot
  become stale after mutation.
- 2026-07-24: Required retained received wire evidence to remain exact and
  immutable across correction, separated integrity observations from correction
  outcomes, and prohibited successful recovery from being treated as proof of
  received, transmitter-original, or semantic validity.
- 2026-07-24: Distinguished complete, incomplete, and malformed parser outcomes;
  required zero stateless consumption on incomplete input, explicit stateful
  accepted-versus-completed ownership, and protocol-justified discard or
  resynchronization.
- 2026-07-24: Required finite repository-owned parser budgets for applicable
  frame, count, nesting, expansion, retained-input, allocation, and aggregate
  dimensions, with explicit finite overrides and no input-controlled bypass.
- 2026-07-24: Defaulted extensible unknown wire values to lossless preservation,
  required attributed layered protocol corpora, and made adopted codec versions
  and features explicit repository pins rather than one global standards pin.
- 2026-07-25: Reviewed the complete eight-video CodeAesthetic catalog and
  adopted qualified guidance for domain naming, legible control flow,
  composition, explicit dependencies, abstraction cost, functional
  transformations, and measured optimization. Rejected blanket bans on
  comments, abbreviations, nesting, `else`, loops, and trait-free or
  trait-required designs; strengthened R163 to preserve durable rationale.
- 2026-07-25: Reviewed Let's Get Rusty `How to write peak Rust` and added
  R207-R210 for enum state modeling, single validated construction paths, exact
  quantity representation, and type-state mechanics. Adopted the first two as
  defaults, kept type-state conditional on a consequential misuse, and rejected
  the video's ordering in which type-state is a universal third level.
- 2026-07-18: Completed repository and cross-agent research; resolved the roles
  and locations of `rsl-deps` and `bitsandbytes`, confirmed caller-controlled
  Rayon policy, and narrowed discovery/precedence questions to adapter drift
  verification.
- 2026-07-18: Confirmed dual MIT/Apache licensing, a twelve-month MSRV window,
  Apple Silicon-first macOS support, material dependency-change approval, and
  Markdown-first canonical skills; reserved testing, examples, and nonmechanical
  code style for dedicated refinement before skill implementation.
- 2026-07-18: Approved the proposed architecture for Stage 2A refinement and
  added the requirement that the standards bundle remain independently
  versioned and relocatable if it is later hosted beneath or consumed by RSL.
- 2026-07-18: Confirmed testing refinement round 1 covering risk-based coverage,
  test boundaries and structure, semantic command tiers, Cargo feature matrices,
  flake handling, fixture regeneration, and semantic assertions.
- 2026-07-18: Completed testing refinement with regression preservation,
  conformance suites, compile-fail contracts, isolated performance evidence,
  continuous fuzzing, native platform execution, testing-dependency policy, and
  resource-limit coverage.
- 2026-07-18: Confirmed example refinement round 1, including purpose-specific
  `examples/` targets distinct from tests, documentation-layer placement,
  compilation policy, non-panicking error handling, progressive cost-aware
  teaching, protocol and DSP examples, and mechanical drift prevention.
- 2026-07-18: Completed example refinement with source-level purpose contracts,
  illustrative rather than exhaustive assertions, production-shaped public API
  use, dependency and feature isolation, inventory maintenance, canonical
  invocation, external-resource behavior, and benchmark separation.
- 2026-07-18: Confirmed nonmechanical code-style refinement round 1 covering
  match-oriented branching, flat preconditions, explicit control flow, conceptual
  function boundaries, scoped mutation and shadowing, domain naming, module
  organization, and imports.
- 2026-07-18: Completed nonmechanical code-style refinement with meaningful
  exhaustiveness, structured error flow, legible iteration, intentional cloning,
  durable comments, a significant-value macro threshold, local unsafe proofs,
  and narrow visibility and lint exceptions. Stage 2A is complete.
- 2026-07-18: Approved and completed the bounded Stage 3 implementation: dual-
  licensed standalone foundations, core and review skills, templates, std-only
  tooling, generated adapters, and eight isolated eval fixtures. No pilot,
  domain-skill, publication, external-installation, or third-party dependency
  scope was added.
