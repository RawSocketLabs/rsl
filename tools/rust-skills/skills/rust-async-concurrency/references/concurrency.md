# Async and Concurrency

### CORE-DESIGN-003 Keep optional execution models caller-owned

- **Strength:** MUST
- **Applies to:** reusable libraries
- **Directive:** Keep the base API synchronous. Gate Tokio and Rayon integration
  behind explicit features and avoid hidden runtime or global-pool ownership.
  Apply CORE-DEP-004 so the feature name exposes ecosystem coupling truthfully,
  CORE-ASYNC-002 so callers drive or explicitly authorize spawned work, and
  CORE-CONC-001 for optional Rayon execution.
- **Why:** A library that builds its own runtime or reaches for a global pool
  makes an application-level decision on the application's behalf. Nested
  runtimes panic, thread counts multiply across independent dependencies, and a
  caller running a different executor has no way out short of forking.
- **Exceptions:** A repository-specific application may own and require one
  runtime.
- **Mechanical owner:** Cargo features, dependency review, tests.
- **Sources:** Preference R28, R29, R109, R110, R185, R186.

### CORE-ASYNC-001 Analyze cancellation at every race

- **Strength:** MUST
- **Applies to:** `select` operations, timeouts, task abort, future races, dropped
  futures, and equivalent early-exit paths.
- **Directive:** Identify the losing operation and what cancellation does at each
  suspension point. Account for partial reads or writes, consumed messages,
  external side effects, locks, permits, buffers, and state-machine progress.
  State whether the operation can be dropped and restarted without lost or
  duplicated behavior.
- **Why:** Cancellation drops a future between suspension points, not at a
  boundary the author chose, so a partial write, a consumed message, or a
  half-advanced state machine outlives the future that owned it. A framed stream
  resumed after such a drop is desynchronized in a way no later read reports.
- **Non-resumable progress:** Keep progress in durable owned state that a
  replacement operation can resume, complete or roll back the operation before
  racing it, or abandon and reset the affected resource. In particular, do not
  continue a framed protocol stream as synchronized after cancelling a partial
  non-resumable read or write.
- **Task ownership:** Distinguish cancelling a future from dropping a task
  handle. Verify the runtime's actual detach, abort, completion, and blocking-task
  behavior. Retain ownership long enough to observe results and cleanup unless a
  documented supervisor owns that responsibility.
- **Verification:** Inject cancellation immediately before and after relevant
  suspension points and during partial I/O. Assert cleanup, resource return,
  protocol state, error reporting, and whether work may continue in the
  background. Use deterministic scheduling or Loom when synchronization order is
  part of the contract.
- **Exceptions:** Shutdown may intentionally discard partial work only when the
  operation contract permits it and the affected connection, stream, or state is
  conclusively abandoned. An operation documented as cancellation-safe still
  needs task and resource ownership analysis.
- **Mechanical owner:** Deterministic lifecycle and cancellation-injection
  tests, protocol conformance tests, and Loom where scheduling order matters.
- **Sources:** Preference R34, R178; Tokio `select!`, `JoinHandle`, and
  `spawn_blocking` documentation. Runtime method lists are version-specific and
  must be checked against the repository's selected version.

### CORE-ASYNC-002 Return work for the caller to drive

- **Strength:** SHOULD
- **Applies to:** reusable async libraries, drivers, streams, and
  runtime-specific adapters
- **Directive:** Prefer returning an async result, `Future`, `Stream`, or explicit
  driver future so the caller chooses where and when to await, compose, race, or
  spawn it. Keep the reusable core free of ambient runtime lookup and private
  runtime creation. A runtime-specific adapter may return work that requires that
  runtime, but must expose and document the coupling rather than presenting it as
  ecosystem-neutral.
- **Why:** Spawning inside a library moves lifetime, cancellation, and failure
  observation somewhere the caller cannot reach, and ambient-context lookup turns
  the coupling into a runtime panic for any caller holding a different one.
  Returning the work leaves those decisions where the ownership already is.
- **Spawn exception:** Spawn internally only when a documented background
  lifecycle is intrinsic to the abstraction and returning all work to the caller
  would not express the required supervision. For Tokio-specific work, accept an
  explicit caller-provided `Handle` or spawner instead of calling
  `Handle::current` or relying on `tokio::spawn` to capture ambient context.
  Store or clone the handle only for the declared lifetime and purpose.
- **Ownership contract:** Return or retain an owned task/supervisor handle and
  define who observes completion, output, error, panic, cancellation, and
  cleanup. State whether drop aborts, joins, requests shutdown, detaches, or
  leaves work running. Do not discard a `JoinHandle` unless another documented
  supervisor assumes these responsibilities.
- **Blocking and local work:** Apply the same explicit ownership to
  `spawn_blocking`, local executors, and runtime-bound resources. Do not move
  sustained DSP compute onto executor blocking pools; follow the repository's
  dedicated compute policy.
- **Exceptions:** An application crate may deliberately own a fixed runtime and
  use its ambient context according to local policy. A framework callback that
  guarantees an entered runtime or local executor may use that guarantee when
  the constraint and panic behavior are part of the integration contract.
- **Mechanical owner:** Public API and feature inspection, construction outside
  ambient runtime context, deterministic task lifecycle and shutdown tests, and
  cancellation-injection tests.
- **Sources:** Preferences R23-R25, R34, R178, R185, and R186; Tokio
  `Handle`, `spawn`, `JoinHandle`, and `spawn_blocking` documentation.

### CORE-ASYNC-003 Own and join production work

- **Strength:** MUST
- **Applies to:** components and repositories that spawn tasks, threads,
  blocking work, local tasks, or long-lived workers
- **Directive:** Keep production work owned and joinable. For every work class,
  identify who starts and owns it, its task/thread/supervisor handle, how
  admission stops, the shutdown signal, drain-or-discard behavior for queued and
  in-progress work, resource and buffer return, join deadline, timeout fallback,
  and who observes completion, errors, and panics.
- **Why:** Work nobody joins cannot be shut down, only outlived. Its failures go
  unobserved, its buffers are still in use while teardown reclaims them, and
  process exit truncates it at an arbitrary point — in practice, mid-write.
- **Default sequence:** Keep production work owned and joinable. Stop admission,
  signal shutdown, apply the repository's declared drain-or-discard policy,
  return resources, and wait for completion within a declared bound. Define the
  consequences of aborting, force-closing, detaching, terminating, or reporting
  incomplete shutdown when the deadline expires.
- **Local policy:** Do not impose a universal drain rule. Decide from accepted-
  work promises, resumability, data-loss or duplication consequences, and the
  precedence of graceful completion versus a shutdown deadline.
- **Detachment:** Dropping a handle is not a lifecycle policy. Transfer work to
  an explicit supervisor or keep it joinable. Truly unjoined process-lifetime or
  best-effort work requires repository approval, bounded resources, observable
  failure handling, and documented process-exit behavior.
- **Exceptions:** Scoped concurrency may encode ownership and joining directly.
  Test-only fault injection and process-terminating paths may use narrower
  handling when lifetime and cleanup consequences are explicit.
- **Mechanical owner:** Deterministic clean, busy, saturated-return-path, error,
  panic, and missed-deadline shutdown tests. Assert admission closure, declared
  completion or loss, resource return, escalation, join results, and no leaked
  background work without timing sleeps.
- **Sources:** Preferences R34, R178, and R186.

### CORE-CONC-001 Keep Rayon execution caller-owned and measured

- **Strength:** MUST
- **Applies to:** Rayon-enabled reusable libraries and parallel
  performance-sensitive paths
- **Directive:** Keep a sequential API and correctness path. Make a
  Rayon-specific parallel entry point accept a caller-owned
  `&rayon::ThreadPool` and execute parallel iterators, joins, or scopes inside
  `ThreadPool::install`. Do not initialize, configure, or silently rely on
  Rayon's global pool from reusable library code.
- **Why:** The global pool is process-wide, so a library that configures it sizes
  threads for an application it cannot see. Parallelism below the workload's
  threshold also costs more in coordination than it recovers, and an unmeasured
  grain size fails on exactly the hardware the benchmark never ran on.
- **Abstraction boundary:** Exposing `ThreadPool` is truthful under a `rayon`
  feature. Introduce a custom executor abstraction only when a demonstrated
  second backend or repository boundary requires substitution; do not hide one
  concrete backend behind a speculative trait.
- **Granularity:** Select the parallel path only above a workload-specific
  threshold supported by benchmarks across representative sizes, targets, pool
  widths, and production features. Name the threshold's unit and record its
  evidence and supported hardware assumptions. Do not publish one universal
  grain-size constant; expose configuration only when real consumers need
  materially different cutoffs.
- **Correctness and nesting:** Run the same conformance suite against sequential
  and parallel paths, preserving required ordering and the declared numerical
  contract. Analyze calls made from an existing Rayon pool: `install` into
  another pool may yield and interleave work on the waiting pool. Prevent
  uncontrolled nesting, oversubscription, hidden reordering, and recursive
  parallelization.
- **Exceptions:** An application crate may deliberately own and configure the
  global pool under local policy. Benchmark and diagnostic hooks may force each
  path to compare them, but ordinary adaptive execution still uses the measured
  threshold.
- **Mechanical owner:** Caller-pool API tests, shared sequential/parallel
  conformance tests, size-sweep benchmarks with recorded pool width and hardware,
  and nested-execution review.
- **Sources:** Preferences R39-R41, R109, and R110; Rayon `ThreadPool`,
  `ThreadPool::install`, and `ThreadPoolBuilder` documentation.

### CORE-CONC-002 Quantify production queue capacity

- **Strength:** MUST
- **Applies to:** production data-path queues, channels, admission buffers, and
  reusable-buffer pools
- **Directive:** Give every capacity a named unit and derive it from a declared
  burst, throughput, memory, or latency requirement. Record the limit in items
  and translate it into worst-case retained bytes and queue-time implications at
  the declared rates. Do not copy a universal capacity into unrelated
  repositories or pipelines.
- **Why:** A capacity copied from elsewhere is a memory and latency budget nobody
  computed. An item count bounds nothing when item size is unbounded, so the
  failure arrives as an out-of-memory kill under the exact load the queue was
  introduced to survive.
- **Memory accounting:** Include owned payload allocations, shared backing
  storage retained by queued handles, queue metadata, reserved permits, and
  other material in-flight buffers. An item count is not a memory bound when
  item size is unbounded; enforce a byte/payload limit or document the separate
  invariant that bounds it.
- **Time accounting:** State the assumptions used to turn depth into time.
  Minimum service rate bounds backlog drain and FIFO waiting; arrival and service
  rates together bound fill time during overload. If the consumer may stall
  indefinitely, do not claim a finite queue-time guarantee merely from capacity.
- **System contract:** Account for the sum of material queues in an end-to-end
  path, and pair capacity with the repository's overload, backpressure, drop,
  coalescing, rejection, and observability policy. Validate configurable
  capacities against repository budgets.
- **Exceptions:** Low-volume control traffic may cite another concrete invariant
  that bounds retained work, memory, and delay. A zero-capacity rendezvous or
  externally bounded queue should document that mechanism rather than inventing
  a numerical budget.
- **Mechanical owner:** Repository adoption decisions, named configuration and
  validation, below/at/above-limit tests, deterministic overload tests,
  occupancy/full-event observability, and load tests where rates matter.
- **Sources:** Preferences R26, R27, R35, R108, and R137.

### CORE-CONC-003 Specify queue semantics before selecting an implementation

- **Strength:** MUST
- **Applies to:** production queues, channels, mailboxes, admission buffers, and
  reusable-buffer pools
- **Directive:** Define the required capacity unit, full-queue outcome, ordering,
  producer/consumer topology, blocking or async wakeup, material fairness,
  cancellation, closure, shutdown/draining, buffer-return, and observability
  semantics before selecting a crate or primitive. This skill does not bless one
  implementation globally.
- **Why:** Queue implementations differ in exactly the properties that decide
  behavior under overload — full-queue outcome, ordering, wakeup fairness,
  closure, cancellation — and none of them is disclosed by the API name.
  Choosing the crate first means learning its real semantics from an incident.
- **Selection boundary:** Repository onboarding or material dependency review
  selects the implementation against the repository's runtime, synchronization
  model, targets, MSRV, dependency and unsafe policy, maintenance constraints,
  and required semantics. Record the choice and any adapter-owned mismatch in
  repository-local decisions.
- **Composite behavior:** Encapsulate nontrivial drop-oldest, coalescing,
  recycling, discontinuity attachment, or multi-step shutdown behind a small
  domain queue type. Make outcomes such as accepted, replaced, rejected, closed,
  or backpressured explicit. Do not duplicate ad hoc `try_send`/receive/retry
  sequences whose atomicity and race behavior are unclear.
- **Exceptions:** Use a primitive directly when it clearly supplies the complete
  local contract and the logic is not duplicated. A repository may standardize
  one implementation locally; that is not an organization-wide default. Do not
  create a generic abstraction in anticipation of an unproven second backend.
- **Mechanical owner:** Below/at/above-capacity tests, exact ordering and
  overload outcomes, deterministic closure and shutdown tests, buffer-return and
  metadata tests, and relevant concurrent-race tests. Verify guarantees from the
  selected implementation's pinned documentation and source rather than its API
  name.
- **Sources:** Preferences R27, R30, R122, R185, and R187.
