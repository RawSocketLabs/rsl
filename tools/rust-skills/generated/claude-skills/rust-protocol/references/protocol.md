# Binary Protocol Engineering

### CORE-PROTO-001 Separate safety limits from selectable validation

- **Strength:** MUST
- **Applies to:** protocol builders, encoders, parsers, decoders, mutation
  tooling, interoperability tests, and intentionally invalid-message workflows
- **Directive:** Default construction to a strict typed `ValidationPolicy` or an
  equivalent named policy owned by the builder. Group selectable checks by
  domain meaning, such as wire conformance and reserved values, integrity,
  canonical representation, and contextual semantics. Use named policy methods
  or profiles; do not use an ambiguous `validate(false)`, positional boolean, or
  unrelated public boolean bag.
- **Why:** Protocol validity and memory safety fail differently: the first
  produces a rejected message, the second produces undefined behavior. A single
  `validate(false)` switch conflates them, so the interoperability workaround
  someone needed on Friday also removes the bounds checks holding back hostile
  input.
- **Non-disableable boundary:** Never let a protocol-validity opt-out disable
  memory safety, bounds checks, checked offset/count/length arithmetic, internal
  representation invariants, or finite frame, nesting, recursion, and allocation
  limits. Repositories may configure documented finite resource budgets, but
  safe input cannot remove the boundary or trigger unchecked indexing,
  overflow, or unbounded allocation. Apply `CORE-PROTO-005` to define and test
  those budgets.
- **Relaxation contract:** Make every relaxation explicit, narrow, and visible
  at construction or parsing. Disabling one group must not disable another.
  Keep structural parsing, integrity status, semantic validity, and application
  trust distinct. A decoder that skips integrity or authentication must not
  return a type that falsely claims trusted validation; preserve raw values and
  status or use a distinctly unchecked result.
- **Lifecycle and encoding:** Apply the selected construction policy during
  `build`, provide explicit validation after construction, and encode the
  represented message faithfully without silently restoring disabled checks.
  Do not assume a built mutable value remains permanently validated. Use
  separate policy types when construction and hostile-input parsing have
  materially different choices.
- **Evolution:** Keep policy representation repository-specific. Private fields,
  named constructors, or another controlled surface should permit new checks
  without forcing every caller to construct a public struct literal. Document
  defaults and treat changes to existing policy meaning as behavior changes.
- **Exceptions:** A small protocol may use a few precisely named builder methods
  instead of a policy type. An authoritative protocol or adopted library may
  provide its own equivalent policy vocabulary. Security-sensitive repositories
  may make integrity or authentication non-relaxable outside dedicated test or
  inspection APIs.
- **Mechanical owner:** Strict-default tests for every validation class; tests
  proving each named relaxation affects only its class; combinations where
  interactions matter; hostile size/offset/overflow/resource tests under every
  policy; faithful invalid-message vectors; post-build validation; mutation and
  encoding tests; and API or compile-fail checks preventing safety-limit bypass.
- **Sources:** Preferences R85 and R90-R93.

### CORE-PROTO-002 Keep policy separate from fresh validation evidence

- **Strength:** MUST
- **Applies to:** built and parsed protocol messages, validation reports,
  integrity and correction status, trusted wrappers, mutation APIs, and
  downstream operations that require validated input
- **Directive:** Treat construction or parsing policy as an input to the
  operation, not part of the message's semantic identity. Do not normally store
  that policy in every built value or include it in message equality, hashing,
  or wire encoding. Return or retain validation evidence only when callers need
  to distinguish what was checked and observed.
- **Why:** Two messages carrying identical wire bytes are the same message
  whatever policy built them, so folding policy into equality, hashing, or
  encoding makes identity depend on construction history. A retained validation
  flag has the opposite failure: it survives the mutation that invalidated it.
- **Evidence:** Distinguish passed, failed, skipped or not checked, and
  inapplicable checks wherever those states change caller behavior. Preserve
  protocol-native evidence such as received integrity status, correction
  outcome, unknown or reserved representation, and received versus corrected
  data when the use case needs it. A `ValidationReport`, status companion, or
  domain-specific equivalent should describe results and skipped groups without
  pretending the policy itself is message data.
- **Trusted boundary:** APIs that require trusted input must validate at their
  boundary or accept a validated domain type or wrapper. The validated form must
  make ordinary safe mutation impossible or restrict mutation to operations that
  preserve its invariant. Any unrestricted mutation invalidates prior evidence
  and requires revalidation; a stale “validated” flag is a defect.
- **Representation choice:** Do not force one generic `Validated<T>` or report
  shape across unrelated protocols. Use a wrapper, separate result, embedded
  protocol status, or immutable domain type according to consumer needs. Avoid
  burdening ordinary strict construction with audit metadata that no consumer
  uses.
- **Persistence:** Serialize validation evidence only when it is part of a
  declared storage, interchange, audit, or protocol contract. State the
  specification revision, validation version, or other context needed to
  interpret persisted results; do not persist an ephemeral builder policy by
  accident.
- **Exceptions:** Immutable messages may store current validation status when it
  is a meaningful domain property. Inspection, forensic, safety-critical, or
  regulated workflows may retain both policy and a full audit report as
  provenance, but that record remains distinct from message identity and must
  stay tied to the exact checked representation. A tiny strict-only builder may
  need no report type.
- **Mechanical owner:** Equality/hash/encoding tests excluding policy; passed,
  failed, skipped, and inapplicable status tests; wrapper or API tests requiring
  validated input; compile-fail or mutation tests proving evidence cannot become
  stale; revalidation tests; received/corrected evidence tests; and persistence
  compatibility tests when reports are serialized.
- **Sources:** Preferences R91, R96, and R195.

### CORE-PROTO-003 Preserve received evidence across correction

- **Strength:** MUST
- **Applies to:** decoders with checksums, CRCs, FEC, erasure recovery,
  descrambling or deinterleaving evidence, damaged-frame inspection, channel
  quality measurement, and interoperability diagnostics
- **Directive:** When received wire data is retained as evidence, keep its exact
  bytes or symbols immutable and lossless. Produce corrected or recovered data
  as a separate result tied to that evidence; do not overwrite the received
  representation in place. Call an observed representation `received`, not
  `original`, unless the transmitter's original data is independently known.
- **Why:** Correcting in place destroys the only record of what the channel
  actually delivered, and that record is the entire input to link-quality
  measurement, interoperability diagnosis, and any later dispute about whether a
  peer or the medium produced the damage. It cannot be reconstructed afterward.
- **Status model:** Keep scoped integrity observations separate from correction
  outcomes. Represent material states such as not checked, passed, and failed
  for the named integrity check, and not attempted, not needed, corrected, and
  uncorrectable for the named recovery operation. A protocol may use separate
  enums, a structured report, or an equivalent domain type. Include counts,
  locations, units, confidence, or an explicit unknown extent only when the
  implementation can report them truthfully.
- **Meaning boundary:** Successful correction does not prove that the received
  representation passed integrity, reproduce unknowable transmitter-original
  data, or establish semantic validity. Likewise, an integrity pass does not
  imply all semantic checks passed. Keep received evidence, recovery outcome,
  post-recovery integrity, and semantic validation independently attributable.
- **Consumer surfaces:** Rich inspection and quality-analysis APIs may expose
  received data, recovered data, and the full report. Ordinary consumers may
  receive only the recovered semantic value plus the status needed to apply
  their trust policy. Treat discarding received evidence as deliberate
  information loss rather than silently relabeling recovered output.
- **Association and mutation:** Bind evidence, recovered output, and status to
  the same frame or message so safe callers cannot accidentally mix them.
  Mutating either representation invalidates or detaches evidence whose scope no
  longer matches.
- **Exceptions:** A strict decoder that rejects every damaged input and has no
  inspection or quality consumer need not retain received bytes. In-place
  correction may be used in a measured constrained path only when the API makes
  the loss of received evidence explicit and no promised consumer requires it.
- **Mechanical owner:** Known-answer tests with exact received and recovered
  representations; immutability or non-aliasing tests; not-checked, passed,
  failed, not-attempted, not-needed, corrected, and uncorrectable cases as
  applicable; truthful correction extent and unit tests; tests proving
  correction success does not rewrite received integrity history; and API tests
  preventing cross-frame evidence association.
- **Sources:** Preferences R96, R195, and R196.

### CORE-PROTO-004 Distinguish incomplete input from malformed input

- **Strength:** MUST
- **Applies to:** stateless parsers, incremental decoders, stream framers,
  buffered protocol readers, resynchronization, and partial-delivery tests
- **Directive:** Make complete, incomplete, and malformed outcomes
  distinguishable in the API. Treat incomplete input as a normal request for
  more data, not as malformed input or an opaque parse error. Keep the concrete
  Rust result type repository-specific when its semantics remain explicit.
- **Why:** The two outcomes demand opposite responses — wait for more bytes, or
  discard and resynchronize. A parser that reports them identically forces the
  caller to guess, and either guess is a failure mode: dropping a frame that had
  simply not arrived yet, or blocking forever on input that will never parse.
- **Stateless consumption:** On a complete result, report the exact consumed
  prefix when trailing data is accepted. On an incomplete result, consume
  nothing and require the caller to retain the full input for retry. If the
  parser reports a needed size, state whether it is exact or a lower bound and
  do not claim more precision than the format permits.
- **Stateful consumption:** A stateful decoder may accept and retain a prefix it
  owns. Distinguish bytes accepted into internal storage from bytes forming a
  completed frame, bound retained input, and make retry behavior free of
  duplication or loss.
- **Malformed and recovery:** Report failure location separately from bytes the
  caller may safely discard. Report a discard or resynchronization count only
  when a protocol-defined marker, length boundary, or other documented
  invariant justifies it. Otherwise leave recovery to explicit local policy.
- **Progress:** Prevent loops that repeatedly return a non-complete outcome
  without accepting input, requesting input, consuming a justified prefix, or
  returning control. Preserve non-disableable size, arithmetic, nesting, and
  allocation limits under `CORE-PROTO-005` while awaiting completion.
- **Exceptions:** A whole-buffer parser for a fixed-size value may use ordinary
  `Result` when buffer length makes truncation directly and unambiguously
  distinguishable in its error type. An adopted parsing framework may use its
  own equivalent outcome and consumption vocabulary.
- **Mechanical owner:** Split every valid frame at every practical byte
  boundary; retry incomplete input and compare with one-shot parsing; verify
  zero stateless consumption on incomplete input; test stateful
  accepted-versus-completed counts, retained-buffer limits, malformed offsets,
  justified resynchronization, trailing data, repeated calls, and zero-progress
  prevention.
- **Sources:** Preferences R87 and R197.

### CORE-PROTO-005 Give hostile-input parsing finite resource budgets

- **Strength:** MUST
- **Applies to:** variable-size frames and messages, recursive or nested
  formats, length- and count-prefixed fields, expansion or decompression,
  incremental buffering, allocation, and attacker-influenced parser work
- **Directive:** Define finite limits for every applicable resource dimension,
  including input or frame bytes, field or item counts, nesting or recursion,
  decoded expansion or output bytes, retained incomplete input, and allocation.
  Document repository defaults and derive them from protocol maxima and
  deployment needs rather than accidental integer or container limits.
- **Why:** A length field is attacker-chosen input. Without a finite budget it
  becomes a direct instruction to allocate, recurse, or wait, so a few bytes on
  the wire buy an out-of-memory kill or a stack overflow. Relying on integer or
  container maxima sets that budget to whatever the type happens to allow.
- **Policy boundary:** Keep resource budgets independent from selectable
  protocol-validity checks. Accept caller overrides only through an explicit
  validated configuration whose values remain finite. Untrusted fields may be
  checked against a budget but must never select, disable, or expand it.
- **Enforcement:** Check declared sizes, counts, cumulative totals, and
  multiplication or addition before indexing, reserving, allocating, recursing,
  or performing proportional work. Bound both individual elements and aggregate
  state where either can grow. Return a structured limit-exceeded result rather
  than continuing to wait for, buffer, or allocate attacker-selected sizes.
- **Lifecycle:** State whether limits apply per field, frame, message,
  connection, decoder instance, or time window and when each budget resets.
  Incremental delivery must not evade a total limit by splitting input across
  calls. Reconfiguration must not strand already-retained data above the new
  budget.
- **Configuration:** Record defaults, units, rationale, hard protocol maxima,
  deployment overrides, and ownership in repository configuration. A public
  library may offer conservative finite defaults and explicit higher finite
  limits without claiming one deployment size fits every consumer.
- **Exceptions:** Fixed-size, statically bounded formats need no runtime policy
  object when their effective limits are evident and tested. Embedded code may
  use compile-time capacities. An authoritative protocol maximum may be a
  non-configurable hard limit.
- **Mechanical owner:** Tests immediately below, at, and above every limit;
  aggregate-versus-per-item cases; checked arithmetic overflow; expansion
  ratios; nesting and recursion; incremental retained-input accumulation;
  reset and reconfiguration; all validation-policy combinations; and proof that
  untrusted input cannot change the active budget.
- **Sources:** Preferences R85, R91, R93, and R198.
