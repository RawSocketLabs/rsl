# DSP and Streaming

## Contents

- `CORE-DSP-001`: propagate and handle sample discontinuities
- `CORE-DSP-002`: keep streaming and receiver terms semantically distinct
- `CORE-DSP-003`: bind stream metadata at continuity-sensitive boundaries
- `CORE-DSP-004`: introduce processing traits only for demonstrated composition
- `CORE-DSP-005`: represent exact rate changes with directed rationals
- `CORE-DSP-006`: separate reset from finite-stream completion
- `CORE-DSP-007`: name monotonic timing events and export durations

### CORE-DSP-001 Propagate and handle sample discontinuities

- **Strength:** MUST
- **Applies to:** lossy sample pipelines, sample queues, buffer transports, and
  stateful DSP stages
- **Directive:** When samples are dropped, overwritten, or otherwise missing,
  bind a discontinuity to the next delivered buffer. Use an equivalent of these
  distinct records rather than unrelated optional fields:
  - A **within-epoch gap** carries the stream epoch, the absolute index of the
    next delivered sample, and a loss extent that is either one known half-open
    range `[start, end)` or explicitly unknown.
  - A **restart or reconfiguration** carries a new epoch and the first delivered
    index in that epoch. Do not fabricate a cross-epoch loss range.
- **Why:** Samples across a gap are contiguous in memory and discontinuous in
  time, and nothing in the buffer distinguishes the two. A stage that treats them
  as adjacent produces a phase jump, a corrupted filter history, and a demodulated
  output that looks plausible enough to reach the consumer unflagged.
- **Evidence and reason:** Treat extent and cause separately. A device overrun,
  queue overflow, source restart, retune, rate change, counter gap, or unknown
  reason classifies why continuity ended; it does not by itself prove the lost
  indices. Derive a known missing count from the range with checked arithmetic
  instead of storing inconsistent count and range fields. A zero-length range
  is not loss.
- **Units and accumulation:** Encode or name the index unit, origin, and
  rollover behavior, including whether positions count per-channel samples,
  interleaved scalars, sample frames, symbols, or another quantity. Coalesce
  repeated losses before delivery only when evidence proves one exact
  within-epoch union; otherwise retain multiple ranges when the API supports
  them and consumers need them, or mark the aggregate extent unknown. Never
  widen an exact claim by guessing.
- **State policy:** Every stateful consumer must declare whether a discontinuity
  causes reset/reinitialization, an error, or a domain-justified gap-aware
  continuation. Reset before processing the next delivered samples by default;
  never silently bridge a gap with stale filter, synchronization, decoder, or
  timing state. A pipeline requiring full continuity must use backpressure or
  fail explicitly instead of dropping samples.
- **Propagation:** Forward or transform the discontinuity through downstream
  stages. Rate-changing stages must define how input indices and loss map to
  output metadata. Mark warm-up or reacquisition output invalid or degraded when
  reset does not immediately restore the normal numerical contract.
- **Observability:** Count drops and resets at a bounded reporting boundary, but
  do not substitute aggregate metrics or wall-clock timing for the per-stream
  discontinuity carried with data.
- **Exceptions:** A stateless stage may simply forward the metadata. A
  mathematically gap-aware stage may preserve selected state only when its
  contract and tests establish that behavior. A terminal sink need not propagate
  metadata further, but must apply its declared handling policy.
- **Mechanical owner:** Deterministic loss injection before and across buffers;
  known, unknown, repeated, and zero-length extent tests; checked range/count
  arithmetic; new-epoch restart, retune, and rate-change tests; multiple-loss
  coalescing; fresh-state equivalence after reset; downstream range mapping; and
  overload observability.
- **Sources:** Preferences R28, R56, R102-R106, and R137.

### CORE-DSP-002 Keep streaming and receiver terms semantically distinct

- **Strength:** SHOULD
- **Applies to:** SDR, DSP, streaming, receiver, and protocol-facing APIs,
  documentation, tests, metrics, and reviews
- **Directive:** Read the repository glossary first and use one term for one
  concept. When the repository has not chosen another vocabulary, use these
  defaults:
- **Why:** Words like sample, block, chunk, and frame name different quantities
  at different stages, and a mismatch between two of them is a silent unit error
  the compiler cannot see. The result is an off-by-a-factor rate, capacity, or
  latency computation that looks correct in every review.
  - A **sample** is one scalar or complex measurement at one instant; an **IQ
    sample** is one complex in-phase/quadrature measurement. State the
    representation when “sample” could be ambiguous, and count complex IQ sample
    rates in complex measurements per second.
  - **Native IQ** is a device or file representation plus the normalization
    metadata needed to interpret it. A **capture** is acquired sample material,
    possibly across blocks, and is not inherently gap-free.
  - A **chunk** is one API-call-sized piece of a continuous stream; arbitrary
    chunk boundaries must not alter a chunk-independent stage's result. A
    **block** is one complete contiguous span and is a natural ownership-transfer
    unit. Bind explicit start/continuity metadata when the block participates in
    a continuity-aware stream contract. A **dwell** is an application-selected
    observation interval for a tuning or candidate, not a generic DSP buffer.
  - A **discontinuity** is missing stream time or changed geometry. Apply
    `CORE-DSP-001`; do not present it as ordinary adjacency.
- **Stage vocabulary:** Keep spectral discovery, channelization, demodulation,
  symbol recovery, synchronization, detection, and decoding distinct. A
  channel is one filtered complex-baseband signal; a selected channel also
  carries the rate, passband, guard, and related input contract. Demodulation
  produces a modulation-domain waveform, symbol recovery produces decisions at
  the symbol clock, synchronization matches or tracks patterns over symbols,
  detection selects among supported hypotheses or receiver paths, and decoding
  frames, corrects, validates, and interprets wire structure.
- **Evidence vocabulary:** Use **received** for exact pre-correction wire
  evidence and **recovered** for a value after signal recovery or FEC/EDAC,
  qualifying the layer when needed. Keep observed evidence distinct from caller
  configuration and durable scheduling state. A detector reports evidence or a
  conclusion; retuning, persistence, and policy remain caller or orchestrator
  actions unless a broader controller API says otherwise.
- **Repository mapping:** Record any different definitions, multichannel unit
  such as a sample frame, and mappings from local public types to the glossary.
  Repository terms override these defaults. Do not rename a stable public API
  mechanically; migrate terminology across code, rustdoc, guides, examples,
  tests, errors, metrics, and compatibility material as one reviewed change.
- **Exceptions:** Follow an authoritative protocol's exact terminology within
  its layer and explain collisions with broader DSP terms. A block-sensitive
  algorithm may define an algorithmic block distinct from a transport block.
  Historical and compatibility documentation may retain old names when labeled.
- **Mechanical owner:** Repository glossary and onboarding decisions, public API
  and documentation review, chunking-equivalence tests, continuity tests, and
  semantic-version or consumer checks for public renames.
- **Sources:** Preferences R64, R97, R102, R103, R156, and R188; reviewed RSL
  `libsdr` vocabulary at `15dc4625e1dea2ae64e800a83ade78f24090be36`.

### CORE-DSP-003 Bind stream metadata at continuity-sensitive boundaries

- **Strength:** MUST
- **Applies to:** sample acquisition, capture, queues, streaming transports,
  continuity-sensitive stages, and rate- or channel-changing boundaries
- **Directive:** Do not force every owned buffer to carry every possible field.
  A simple finite or offline block may own only contiguous data. Once an API
  claims stream position, adjacency, rate, channel geometry, or discontinuity
  behavior, bind the payload to the metadata required to uphold that claim so
  they cannot be reordered, dropped, or paired independently by ordinary safe
  use.
- **Why:** Metadata carried beside its payload can be reordered, dropped, or
  paired with the wrong buffer by ordinary safe code, and the mistake surfaces as
  a plausible signal rather than an error. Attaching everything everywhere has
  the opposite cost: fields that no stage establishes get filled with guesses.
- **Boundary metadata:** Select fields from the actual contract rather than a
  universal envelope. A continuity-aware sample block normally needs a stream
  epoch or restart identity, first-sample position, declared index unit, sample
  rate, and discontinuity state. Add block sequence, channel/selection geometry,
  source timestamp, or diagnostic timing only when the consumer contract uses
  them. Apply `CORE-DSP-001` to loss.
- **Representation:** Use a domain block, envelope, borrowed view, or explicit
  context argument that preserves the association. Do not require kernels to
  retain irrelevant transport metadata: borrow the samples and the narrow
  context a kernel needs while the surrounding stage continues to own
  propagation. Do not keep payload and authoritative metadata in parallel
  queues, unrelated optionals, or independently mutable records.
- **Construction and transformation:** The boundary that can establish
  provenance creates the metadata; downstream code must not invent it. Do not
  expose incomplete or discontinuous data through an accessor that promises a
  complete continuous block. A stage that changes rate, channel selection,
  length, or time mapping creates corresponding output metadata rather than
  copying stale input values.
- **Exceptions:** A finite algorithm with an explicit call-local rate or start
  may use a data-only block or slice. Stateless kernels may receive no metadata
  when their output is independent of it. Named unchecked or assumption-based
  entry points may omit proof only under repository policy and must not
  fabricate continuity on output.
- **Mechanical owner:** Type and constructor review; compile-fail or API tests
  preventing metadata/payload separation where material; complete, partial,
  discontinuous, reorder, restart, rate-change, and channel-change tests.
- **Sources:** Preferences R99, R102-R106, R188, and R189; reviewed RSL
  `libsdr` separation of `IqBlock`, timed sample buffers, and selected-channel
  buffers at `15dc4625e1dea2ae64e800a83ade78f24090be36`.

### CORE-DSP-004 Introduce processing traits only for demonstrated composition

- **Strength:** SHOULD
- **Applies to:** traits and trait objects for DSP, audio, streaming, parsing, or
  receiver stages
- **Directive:** Keep concrete processor types and static generic composition by
  default. Introduce a shared trait only when multiple real implementations
  perform one coherent operation at a demonstrated composition boundary. Do not
  create a universal `Processor`, `Stage`, or `Transform` merely because
  unrelated types can be made to share a method shape.
- **Why:** A shared method shape is not a shared contract. A universal processor
  trait forces every implementation to accept the widest input, ownership, and
  error model any member needs, erases the type distinctions that made invalid
  pipelines unrepresentable, and puts dynamic dispatch on a per-sample path.
- **Contract:** Define the family's input representation and ownership, output
  ownership or destination, consumed and produced units, output-capacity or size
  bounds, algorithmic latency, internal buffering, empty-input and arbitrary-
  chunk behavior, reset, discontinuity handling, flush/end-of-stream behavior,
  error taxonomy, and state after error. A concern may be inapplicable, but it
  must not remain accidentally unspecified. Keep materially different
  in-place, rate-changing, framing, or sink semantics in separate capabilities.
- **Dispatch choice:** Use generics and associated types when the implementation
  is chosen statically. Prefer an enum when a closed set needs runtime
  selection. Use a trait object only for genuinely open or runtime-configurable
  heterogeneity such as plugins, runtime reordering, or application-selected
  stage types. Keep dynamic dispatch outside measured per-sample kernels where
  practical and measure it when the cost could decide the design.
- **Object boundary:** Design object safety deliberately rather than weakening
  types after the fact. A dynamic adapter may normalize richer concrete APIs
  into an object-safe contract, but must preserve counts, capacity, metadata,
  errors, and lifecycle. Do not hide allocation or clone buffers merely to make
  object erasure convenient.
- **Evolution:** Treat public required methods, associated types, supertraits,
  object-safety, implementor freedom, and default behavior as compatibility
  commitments. Seal a trait when implementations must stay under repository
  control; otherwise test at least one external-style implementor.
- **Exceptions:** A framework-owned callback trait or stable foreign interface
  may require an adapter even with one local implementation. A narrow internal
  test trait may be simpler when its scope and contract remain obvious.
- **Mechanical owner:** Shared conformance tests for every implementation,
  one-shot/chunked and reset/flush/error-state tests, output-bound tests, public
  consumer fixtures, semantic-version checks, and concrete-versus-dynamic
  equivalence or benchmarks when both paths exist.
- **Sources:** Preferences R17, R101-R105, and R190.

### CORE-DSP-005 Represent exact rate changes with directed rationals

- **Strength:** MUST
- **Applies to:** decimators, interpolators, rational resamplers, clock-domain
  conversions, framers, and metadata or capacity calculations across rates
- **Directive:** Represent an exact constant rate relationship as a positive,
  reduced rational in the explicit direction `output/input`. Name numerator and
  denominator for that direction, or document an established `L/M` convention
  where `output_rate = input_rate × L/M`. Keep absolute input and output rates
  as separate domain values and verify that any supplied pair agrees with the
  relationship.
- **Why:** A bare ratio does not say which direction it converts, so an inverted
  interpretation still type-checks and still produces output — at the wrong rate.
  A floating-point ratio adds drift that accumulates into a position error over a
  long stream, which is precisely where nobody is watching for it.
- **Arithmetic:** Reject zero terms and normalize by the greatest common divisor.
  Use checked integer arithmetic for lengths, capacities, positions, durations,
  and metadata mappings; do not use a floating-point approximation to establish
  an exact allocation or index bound. Report overflow or an unrepresentable
  result instead of wrapping, saturating silently, or allocating from a truncated
  value.
- **State-aware sizing:** Account for retained fractional phase, buffered input,
  pending output, startup latency, and flush/tail policy. Provide equivalents of
  `max_output_for(input_len)` and `input_needed_for(output_len)` when callers
  allocate or schedule work; name whether a result applies to current state,
  reset state, steady state, or final flush. Return the actual consumed and
  produced counts from processing when they are not otherwise unambiguous.
- **Chunking:** Carry fractional phase and history across calls. Do not round an
  ideal per-chunk count independently, because repeated rounding can make output
  depend on transport chunk boundaries. Map positions and discontinuities
  through the same exact relationship and declare latency/reference offsets.
- **Variable-rate exception:** An adaptive or asynchronous converter whose
  relationship changes over time must expose its time base, estimator/control
  state, supported range, and conservative sizing bounds. Do not label an
  estimate as an exact rational contract.
- **Exceptions:** An authoritative specification may supply equivalent
  numerator and denominator names, but the API and documentation must still
  state their direction. A variable-rate converter follows the contract above
  instead of claiming one constant exact relationship. A higher-precedence
  repository decision may use an equivalent directed, normalized
  representation; it may not replace exact checked bounds with an
  approximation.
- **Mechanical owner:** Constructor reduction and zero/overflow tests; property
  tests for checked floor/ceil bounds; one-shot versus partitioned count and
  sample equivalence; state-dependent capacity tests around phase boundaries;
  flush and discontinuity tests; absolute-rate and metadata mapping checks.
- **Sources:** Preferences R97, R102-R106, and R191; reviewed RSL `libsdr`
  `F_out = F_in × L/M` vocabulary and streaming phase behavior at
  `15dc4625e1dea2ae64e800a83ade78f24090be36`.

### CORE-DSP-006 Separate reset from finite-stream completion

- **Strength:** MUST
- **Applies to:** stateful streaming processors, filters, resamplers, framers,
  encoders, decoders, parsers, and pipelines with buffered or delayed output
- **Directive:** Give reset and finite end-of-input distinct contracts. Reset
  discards buffered input, pending output, fractional phase, history, and other
  stream-local progress without emitting it, then restores the documented
  initial state while preserving configuration unless the API says otherwise.
  Empty input is not an implicit reset or end-of-stream signal.
- **Why:** Reset discards buffered state and completion emits it, so conflating
  them either drops the filter tail a finite stream was owed or injects a
  synthetic tail into a live stream that has not ended. Both produce output that
  passes every length check.
- **Finish or terminal flush:** Treat `finish`, `close`, or a terminal `flush`
  as a declaration that no more input belongs to this finite stream. Emit every
  remaining output justified by the declared tail policy, or return the
  declared incomplete-tail error. State whether the policy drops an incomplete
  tail, emits a semantically valid partial result, or pads or otherwise
  synthesizes input. Repeated completion must not duplicate output: it is either
  idempotent after all output is observed or reports the already-finished
  state. Reject further input until reset or explicit reinitialization.
- **Tail provenance:** Keep received input distinct from padding, extrapolation,
  filter warm-down, or other synthetic tail material. Preserve a valid-input
  length, provenance marker, degraded interval, or equivalent domain evidence
  wherever a consumer could otherwise interpret generated tail values as
  received data.
- **Live streams and shutdown:** Do not finish an infinite or live stream at
  ordinary chunk boundaries. The owner of shutdown chooses explicitly whether
  to stop admission and finish/drain the finite prefix or discard/reset pending
  state; apply `CORE-ASYNC-003` when concurrent work is involved. If a stage can
  expose currently ready output without declaring end-of-stream, name that
  operation separately from finish.
- **Exceptions:** A stateless stage with no buffered or delayed output may omit
  reset and finish. A stage with no possible tail may implement completion as a
  documented no-op state transition. Protocol or transactional APIs may use
  domain terms such as `finalize`, `commit`, or `close`, but must retain the
  reset-versus-completion distinction and their domain-specific error behavior.
- **Mechanical owner:** Fresh-instance equivalence after reset; proof that reset
  emits nothing; one-shot versus arbitrarily chunked completion; tail-length
  classes; repeated completion; input after completion; reset and reuse after
  completion; padding/provenance checks; and explicit drain-versus-discard
  shutdown tests.
- **Sources:** Preferences R102 and R192; `CORE-ASYNC-003` for concurrent
  shutdown ownership.

### CORE-DSP-007 Name monotonic timing events and export durations

- **Strength:** MUST
- **Applies to:** streaming metadata, queue and pipeline instrumentation,
  latency measurement, diagnostic events, and APIs that expose monotonic time
- **Directive:** Do not attach an ambiguous `timestamp` or `Instant` to every
  buffer. Name the event being measured, such as acquisition start or
  completion, enqueue acceptance, dequeue receipt, processing start or
  completion, successful handoff, or transmission start or completion. Capture
  time at that event; do not relabel an attempt or pre-await sample as successful
  completion.
- **Why:** An unnamed timestamp cannot be subtracted from another one safely: the
  two may mark different events, different clock domains, or an attempt rather
  than a completion. The resulting latency figure is precise, plausible, and
  measuring something nobody chose.
- **Clock domains:** Use sample positions, rates, epochs, and discontinuities as
  authoritative stream continuity. Keep hardware/source time, monotonic
  operational time, and wall-clock correlation in distinct fields or types with
  their clock, epoch, and uncertainty stated. Timing delay alone does not prove
  sample loss.
- **Representation:** Treat `std::time::Instant` and an equivalent monotonic
  handle as process-local and opaque. Compare named endpoints from the same
  clock domain and expose, aggregate, or persist the resulting `Duration`,
  counters, or histogram values. Do not serialize an `Instant`, use it as a
  cross-process timestamp, or imply a calendar epoch. Use checked duration
  calculation when reversed event order or clock anomalies must remain visible.
- **Placement and cost:** Keep operational timing in a diagnostic sidecar or
  boundary instrumentation unless the consumer's semantic contract needs it.
  Do not burden every payload or hot-loop iteration with unused timing. Measure
  capture and aggregation overhead when it could affect the latency being
  observed, and use bounded sampling or aggregation under repository policy.
- **Exceptions:** An authoritative hardware or protocol timestamp may be
  semantic payload metadata; retain its exact clock-domain contract rather than
  replacing it with `Instant`. Cross-process or persistent correlation may use a
  declared external clock such as `SystemTime`, with its non-monotonic behavior
  and uncertainty handled explicitly. `no_std` targets may use a supplied tick
  source or monotonic-clock trait with equivalent event and scope semantics.
- **Mechanical owner:** Fake-clock or deterministic event-order tests; blocked,
  failed, and successful handoff cases; endpoint and duration-label checks;
  serialization-schema checks excluding process-local handles; clock-domain
  conversion tests where supported; and measurement-overhead evidence for hot
  paths.
- **Sources:** Preferences R106 and R193; Rust standard-library `Instant`,
  `Duration`, and `SystemTime` documentation, reviewed at Rust 1.97.1.
