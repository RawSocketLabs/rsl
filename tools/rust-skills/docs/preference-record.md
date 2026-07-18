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
  path.

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
- Whether a built message records which validations were skipped.
- Whether encoding revalidates a message after construction or respects the
  builder's validation policy.
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

- Task/thread ownership, shutdown, joining, draining, and discard behavior are
  application-specific decisions.
- Repositories applying the general standards should answer these questions
  explicitly rather than inheriting one universal drain policy.

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

- **Strength:** SHOULD
- **Scope:** async and threaded production pipelines
- **Rule:** Use bounded channels and select capacity from an explicit memory,
  burst, throughput, or latency rationale.
- **Rationale:** bounded queues expose overload and place a limit on memory and
  queueing latency.
- **Acceptable exceptions:** low-volume control traffic whose possible growth is
  demonstrably bounded by another invariant; record that invariant near channel
  creation.
- **Review questions:** What happens when the queue fills? How much memory and
  latency can the configured capacity retain?

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
  consumer works on fresher data.
- **Rationale:** stale samples increase end-to-end latency and may be less useful
  than current samples.
- **Acceptable exceptions:** an algorithm requires continuity or complete sample
  history; in that case apply backpressure or fail the stream explicitly.
- **Review questions:** How are discontinuities signaled to stateful DSP stages?
  Are dropped sample counts observable?

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
- **Rule:** State who owns spawned work, how cancellation or shutdown is
  signaled, whether queued work drains or is discarded, how resources and
  buffers are returned, and which work is joined.
- **Rationale:** shutdown correctness depends on application semantics and cannot
  be selected safely by a universal Rust rule.
- **Acceptable exceptions:** none when spawned work can outlive the initiating
  scope; a simple scoped thread construct may encode ownership directly.

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

### Unresolved decisions

- Whether optional features should be named after capabilities (`async`,
  `parallel`) or ecosystems (`tokio`, `rayon`).
- Whether Tokio adapters accept an existing runtime handle or remain runtime-
  agnostic futures that the caller spawns.
- Exact caller-provided Rayon pool API and measured minimum grain-size contract.
- Queue capacity selection and whether repositories must state numerical memory
  or latency budgets.
- How drop-oldest sample queues signal gaps and reset or preserve DSP state.
- Which queue and channel crates best express drop-oldest and recycle behavior.
- Repository-template questions for shutdown, cancellation, draining, joining,
  and detached work.

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
  syntax. Give a significant `TODO` or `FIXME` enough context plus a tracking
  reference or removal condition to make the deferred work actionable.
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
- Separate transport buffering, framing, structural decoding, integrity checks,
  semantic validation, and application interpretation conceptually.
- Simple protocols may combine layers in code, but their responsibilities and
  error locations remain distinguishable.
- Distinguish incomplete input from malformed input.
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

- Give the builder a named validation policy with all checks enabled by default.
- Apply the policy during `build()`.
- Do not claim that the resulting owned message remains permanently validated.
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
- When correction occurs, expose whether correction happened, its extent, and
  received versus corrected representations when relevant.

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
  without panicking.
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
  structurally invalid frame. Preserve enough state or consumption information
  for the caller to continue safely.
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
- **Rule:** Represent unknown values losslessly, such as `Unknown(raw)`, when
  forward compatibility, proxying, inspection, or round-trip fidelity matters.
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

- **Strength:** SHOULD
- **Scope:** protocol construction
- **Rule:** Represent construction checks with a named policy owned by the
  builder. Enable all checks by default, apply selected checks during `build()`,
  and provide explicit validation after construction.
- **Rationale:** a named policy makes deliberate invalid construction readable
  and adaptable without splitting the entire model into valid and raw families.
- **Acceptable exceptions:** repository-local rules may choose a different clear
  lifecycle or a simpler builder for a small protocol.

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
  invalid Rust memory, unchecked indexing, impossible internal layout, or other
  soundness failures through safe code.
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
  occurred, its meaningful extent, and original versus corrected data when the
  use case needs both.
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

### Unresolved decisions

- Exact released `bitsandbytes` versions to support and how standards material
  pins or tracks them; repository research confirmed the crates and current
  contracts in `RawSocketLabs/rsl`.
- Standard `ValidationPolicy` shape, naming, and granularity.
- Whether messages retain construction-policy or validation-result metadata.
- Exact incomplete-input result type and byte-consumption contract.
- Default maximum frame, nesting, and allocation limits and where repositories
  declare them.
- Default behavior for unknown values when repository instructions are silent.
- Standard representation for original, corrected, and integrity-status data.
- Fuzz corpus and interoperability-vector sources for protocol implementations.

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
- Allow buffers to carry relevant sample rate, channel, timestamp or sample
  index, discontinuity, capacity, and related metadata.
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

#### Rate-changing stages

- Expose output-size bounds or required capacity before processing for
  decimators, interpolators, resamplers, framers, and similar stages.
- Represent rate relationships explicitly.
- Do not allow surprising steady-state buffer growth in hot paths.

#### Discontinuities and timing

- Mark sample loss or discontinuities explicitly and carry a lost count or sample
  index range when known.
- Require stateful stages to declare whether they reset, continue with degraded
  output, or return an error after discontinuity.
- Optionally attach a monotonic `Instant` or derived interval describing send
  timing relative to the prior sent buffer.
- Treat timing deltas as diagnostic corroboration, not authoritative proof of
  sample loss.

#### Hot-path observability

- Do not log inside per-sample or tight per-block loops.
- Collect cheap measurements and report them at pipeline boundaries.
- Observe dropped samples, queue saturation, processing duration, high-water
  marks, buffer starvation, and allocation fallback where relevant.

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
- **Rule:** Implement `From` for clear, infallible conversions whose result does
  not conceal an important choice. Use named methods for conversions whose
  representation, reference, rounding, normalization, or domain meaning should
  be visible. Use `TryFrom` for validation or failure.
- **Rationale:** standard traits improve ergonomics, while named operations keep
  consequential semantics readable.
- **Acceptable exceptions:** repository vocabulary may establish an unambiguous
  conventional conversion suitable for `From`.

#### R99. Move owned domain buffers through pipelines

- **Strength:** PREFER
- **Scope:** DSP and streaming pipeline boundaries
- **Rule:** Transfer an owned domain buffer that can retain storage and relevant
  stream metadata. Give kernels efficient slice access and provide plain-`Vec`
  adapters for simple consumers.
- **Rationale:** ownership transfer supports buffer reuse without globally shared
  mutation, while a domain buffer carries continuity and timing context.

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
  Define a shared trait only when it expresses a useful contract implemented by
  multiple stages.
- **Rationale:** uniform traits should serve actual composition rather than erase
  meaningful differences among DSP operations.

#### R102. Document stateful streaming behavior

- **Strength:** MUST
- **Scope:** stateful streaming stages
- **Rule:** Define consumed and produced quantities, latency, buffering, chunk-
  boundary behavior, empty input, reset, flush, and end-of-stream behavior.
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
  before processing. Represent rate relationships explicitly and avoid
  unannounced steady-state growth.
- **Rationale:** callers need to size and recycle buffers without speculative
  allocation.

#### R105. Propagate discontinuities explicitly

- **Strength:** MUST
- **Scope:** lossy streaming pipelines
- **Rule:** Mark a discontinuity after dropped samples and include the known loss
  count or sample-index range. Require each stateful consumer to define reset,
  degraded continuation, or error behavior.
- **Rationale:** silently bridging a gap can corrupt filter state, timing,
  demodulation, and downstream interpretation.

#### R106. Keep timing evidence separate from sample continuity

- **Strength:** SHOULD
- **Scope:** streaming metadata and diagnostics
- **Rule:** Optionally record a monotonic send `Instant` or interval from the
  preceding sent buffer. Use it to detect suspicious timing gaps, but do not infer
  exact sample loss from wall-clock delay alone.
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

### Unresolved decisions

- Canonical buffer vocabulary and type shape (`SampleBuffer`, `SampleBlock`, or
  another established term).
- Which stream metadata belongs on every buffer versus optional wrapper/context
  types.
- Standard trait shape, if any, for composing heterogeneous processing stages.
- Representation of rate relationships and fractional production bounds.
- Flush behavior for infinite streams and stages with irreducible tail state.
- Monotonic timestamp capture point and whether to attach `Instant`, a duration,
  or both.
- Standard discontinuity and loss-range representation.
- Metrics facade and how applications opt into tracing or metrics ecosystems.
- Exact minimum grain-size evidence and pool-passing API for parallel DSP.

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
- **Rule:** Let the application select the Rayon pool or equivalent parallel
  execution context and concurrency level. Do not create uncontrolled nested
  parallelism.
- **Rationale:** applications need to coordinate CPU budgets across DSP and other
  workloads.

#### R110. Measure parallelization granularity

- **Strength:** MUST
- **Scope:** parallel DSP implementations
- **Rule:** Establish a representative minimum grain size at which scheduling and
  synchronization overhead are justified. Preserve required output ordering and
  the scalar numerical contract.
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
