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
- **Exceptions:** Generated APIs or established framework conventions may use a
  uniform builder style. Protocol invalid-message tooling may need a flexible
  builder plus explicit validation policy rather than intrinsically valid types.
- **Mechanical owner:** Consumer examples, compile-fail tests for staged
  invariants, compatibility review, and ergonomics evals.
- **Sources:** Preferences R8-R15 and R91.

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
