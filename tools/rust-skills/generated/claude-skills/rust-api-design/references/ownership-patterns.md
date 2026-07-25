# Ownership and API Patterns

### API-BORROW-001 Borrow the optional value instead of an immutable option

- **Strength:** PREFER
- **Applies to:** read-only parameters and accessors where presence is already
  represented by `Option`
- **Directive:** Prefer `Option<&T>` or `Option<&mut T>` over `&Option<T>` for a
  borrowed optional value. Use `Option::as_ref`, `as_deref`, `as_mut`, or
  `as_deref_mut` at the boundary so the callee receives exactly the optional
  capability it needs.
- **Why:** Borrowing the wrapper adds an unnecessary layer and can couple a
  caller to storage as `Option<T>` rather than to the semantic absence or
  borrowed value.
- **Exceptions:** Use `&mut Option<T>` when the operation must inspect, insert,
  take, replace, or preserve the identity of the optional slot. A generic API
  may borrow an option-like structure when that wrapper is itself the subject.
- **Mechanical owner:** Clippy `ref_option` where applicable, API compilation
  tests, the paired mutable-slot eval, and review.
- **Sources:** Preferences R167 and R177; Rust `Option` documentation; Logan
  Smith advisory material.

### API-SHARE-001 Choose sequence ownership from mutation and sharing

- **Strength:** SHOULD
- **Applies to:** owned sequences crossing API, task, cache, graph, or
  long-lived sharing boundaries
- **Directive:** Keep `Vec<T>` for unique ownership, growth, mutation, capacity
  reuse, and ownership transfer. Consider `Arc<[T]>` when a sequence is frozen
  after construction, cloned among independently owned cross-thread consumers,
  and reference-counting cost is justified. Consider `Rc<[T]>` for the same
  topology confined to one thread. Prefer borrowed slices when ownership need
  not transfer.
- **Why:** Slice-backed shared ownership removes spare capacity and mutable
  vector semantics from the contract, while a vector remains simpler and faster
  for unique or reusable buffers.
- **Exceptions:** Do not convert a uniquely owned or recycled hot-path buffer to
  reference counting. Use `Arc<Vec<T>>` only when vector-specific capacity or
  APIs are genuinely part of the shared contract. Measure material clone,
  allocation, and cache costs.
- **Mechanical owner:** Ownership topology review, allocation and clone
  measurements when material, and paired frozen/reusable-sequence evals.
- **Sources:** Preferences R168-R171; Rust shared-ownership documentation; Logan
  Smith advisory material.

### API-STATE-001 Encode mutually exclusive states in one enum

- **Strength:** SHOULD
- **Applies to:** domain and public types whose fields record lifecycle,
  outcome, mode, or another set of states only one of which holds at a time
- **Directive:** When fields are meaningful only in particular combinations,
  replace them with one enum whose variants own exactly the data each state
  carries, so the payload is reachable only from the state it belongs to. Keep
  genuinely independent attributes as ordinary fields. Match exhaustively inside
  the defining crate under `CORE-STYLE-001` so a later state forces a decision
  at each site rather than inheriting a default.
- **Why:** Parallel flags and optional payloads make the representable state
  space the product of the fields rather than the number of real states. A
  `paid` flag beside an optional receipt and an optional error admits paid-with-
  an-error and unpaid-with-a-receipt, so every function that reads the type
  re-derives which combinations are legal and one that guesses wrong is
  indistinguishable from one that is right.
- **Exceptions:** Keep separate fields when the attributes genuinely vary
  independently. A wire, storage, layout-sensitive, or FFI representation may
  retain the flag-and-payload form; convert to the enum at that boundary and
  keep the raw form out of domain logic. A two-state distinction with no
  per-state data may stay a named boolean when an enum adds no information, and
  `Option<T>` is already this pattern for one optional value.
- **Mechanical owner:** Meaningful exhaustive matches inside the defining crate,
  tests that the removed combinations are no longer constructible, round-trip
  tests at any representation boundary, and API review.
- **Sources:** Preferences R11, R12, and R207; Let's Get Rusty advisory source.

### API-VALID-001 Give a validated type one construction path

- **Strength:** SHOULD
- **Applies to:** newtypes and domain wrappers whose type name asserts an
  invariant
- **Directive:** Apply `CORE-API-001` to decide that the invariant belongs in a
  type, then keep the fields private and make one fallible constructor — a
  `TryFrom` implementation or a named `new`, `parse`, or `from_*` returning
  `Result` — the only way to obtain the value. Do not add a public field,
  setter, `DerefMut`, or unchecked constructor that re-admits an invalid value;
  route mutation through a method that re-establishes the invariant. Document
  the invariant on the type and delete the downstream checks it now proves.
- **Why:** The whole benefit of a validated wrapper is that later holders may
  skip the check. A second construction path or an editable field returns the
  invariant to convention while the deleted checks are no longer there to catch
  what slips through, so the failure surfaces wherever the value is finally
  used rather than where it was built.
- **Unchecked construction:** Provide one only for a demonstrated need such as a
  measured hot path, a trusted internal round trip, or deserialization of data
  the repository already validated. Name it `*_unchecked`, keep it as narrowly
  visible as the need allows, document the caller's obligation, and test that
  the checked path rejects what it claims to reject. Treat `Deserialize` as a
  construction path: validate in a custom implementation or through a
  try-conversion rather than letting a derive reconstruct arbitrary fields.
- **Exceptions:** Keep a primitive when the distinction is transient or local.
  Protocol tooling that must build invalid messages uses the explicit validation
  policy in `CORE-PROTO-001` and `API-BUILDER-001` instead of an intrinsically
  valid type.
- **Mechanical owner:** Visibility and API review, constructor rejection tests
  for each invalid class, serialization round-trip and invariant tests, and
  review of any remaining duplicate validation at call sites.
- **Sources:** Preferences R11, R12, R91, and R208; Rust API Guidelines; Let's
  Get Rusty advisory source.

### API-QUANTITY-001 Represent exact quantities exactly and carry their unit

- **Strength:** SHOULD
- **Applies to:** monetary amounts, counts, sizes, offsets, indices, durations,
  and other domain quantities whose value must be exact or whose unit is
  otherwise implicit
- **Directive:** Represent a quantity that must compare, sum, or round-trip
  exactly as an integer in its smallest meaningful unit or as an exact rational,
  and carry the unit, scale, or currency in the type rather than in a parameter
  name or a comment. Do not use binary floating point for an exact discrete
  quantity. Put rounding, overflow, and mixed-unit behavior on the type using
  checked arithmetic, and report an unrepresentable result instead of truncating
  or saturating silently. Use `CORE-DSP-005` for rate relationships.
- **Why:** Binary floating point cannot represent most decimal fractions, so
  equality, summation order, and round trips drift by amounts small enough to
  pass casual tests and large enough to fail reconciliation. An unqualified
  numeric parameter compounds it: cents and dollars, or hertz and kilohertz,
  mix without complaint because nothing in the type distinguishes them.
- **Exceptions:** Measured, estimated, or inherently continuous values such as
  sample data, physical measurements, and statistics are legitimately
  floating point. Keep an external representation when a format, protocol, or
  API requires it and convert at that boundary. Do not wrap a transient local
  scalar whose unit is unambiguous in context.
- **Mechanical owner:** Exactness and rounding property tests, checked-arithmetic
  and overflow tests, serialization round-trip tests, and review of arithmetic
  that crosses units.
- **Sources:** Preferences R203 and R209; `CORE-DSP-005` arithmetic contract;
  Let's Get Rusty advisory source.

### API-BUILDER-001 Use builders and type-state only for real construction needs

- **Strength:** SHOULD
- **Applies to:** multi-field construction, optional configuration, validation,
  staged resources, and public constructors
- **Directive:** Use a builder when named incremental configuration improves
  clarity, defaults, evolution, or validation. Use type-state only when compile-
  time staging prevents a consequential misuse across a small understandable
  state graph. Keep ordinary constructors for simple required data.
- **Why:** Builders can stabilize configuration surfaces, while speculative
  builder layers and type-state multiply types, generics, diagnostics, and
  compile cost without improving real callers.
- **Type-state mechanics:** A method that panics or returns an error because the
  receiver is in the wrong stage is the signal worth evaluating; a
  configuration field is not. When the transition qualifies, name each state
  with a zero-sized marker type, make the state a type parameter rather than a
  runtime field, bind each operation to the impl block for the state that
  permits it, and make each transition consume the value and return the next
  state so a stale handle cannot be reused. Remove the runtime guard the staging
  now proves rather than keeping both, and document the transition graph.
- **Residual runtime state:** Keep a runtime check wherever the real state
  depends on data, peers, timeouts, or failures the type cannot observe. A typed
  open handle proves that connecting happened, not that the peer is still there,
  so operations must still return errors for conditions that arise after the
  transition.
- **Exceptions:** Generated APIs or established framework conventions may use a
  uniform builder style. Protocol invalid-message tooling may need a flexible
  builder plus explicit validation policy rather than intrinsically valid types.
  A state graph that callers must store in collections, name in their own
  signatures, or select at runtime may be clearer as a runtime enum.
- **Mechanical owner:** Consumer examples, compile-fail tests for staged
  invariants, compatibility review, and ergonomics evals.
- **Sources:** Preferences R8-R15, R91, and R210; Let's Get Rusty advisory
  source.

### API-ERROR-001 Separate reusable errors from application reports

- **Strength:** SHOULD
- **Applies to:** public libraries, applications, services, command-line tools,
  and error-context boundaries
- **Directive:** Give reusable libraries stable typed domain errors with sources
  and machine-relevant distinctions. Use `thiserror` only when approved and its
  derive value outweighs dependency cost. Allow applications to use `anyhow` or
  another report type at presentation and orchestration boundaries when callers
  do not need exhaustive programmatic handling. Do not expose opaque boxed or
  report errors from a durable library API without an explicit policy.
- **Why:** Callers need structured behavior, while applications benefit from
  contextual reports without forcing every layer into one error ecosystem.
- **Exceptions:** A small internal tool may use one report type throughout. A
  deliberately opaque plugin boundary may box errors when downcasting and source
  behavior are documented.
- **Mechanical owner:** Public API tests, source-chain tests, dependency review,
  documentation, and SemVer analysis.
- **Sources:** Preferences R21-R24 and R176.
