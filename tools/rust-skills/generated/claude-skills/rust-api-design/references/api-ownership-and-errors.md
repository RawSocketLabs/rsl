# API, Ownership, and Errors

### CORE-API-001 Preserve important invariants in types

- **Strength:** SHOULD
- **Applies to:** public and domain APIs
- **Directive:** Use a domain type when confusing values creates a meaningful
  defect. At a durable trust boundary, parse an untrusted representation once
  into a type whose normal constructors establish the relevant invariant, then
  pass that type downstream instead of repeatedly validating raw data. Prefer
  builders with validation enabled by default and granular, explicit opt-outs
  for protocol-invalid construction. Apply `CORE-PROTO-001`: protocol
  relaxations never disable safety, checked arithmetic, internal invariants, or
  finite resource limits.
- **Why:** Revalidating raw data at each use means the invariant holds only where
  someone remembered to check, and the one path that forgets becomes the defect.
  A type whose constructors cannot produce an invalid value moves that from
  discipline to proof.
- **Exceptions:** Keep a primitive when the distinction is transient or local
  and conversion noise would dominate. Preserve raw representations when
  diagnostics, round trips, unknown values, or intentionally malformed protocol
  tooling require them; keep structural parsing, semantic validation, and
  application interpretation distinguishable.
- **Mechanical owner:** Tests and review.
- **Sources:** Preference R11, R12, R91, R173; Rust API Guidelines.

### CORE-API-002 Keep public surfaces small and conventional

- **Strength:** SHOULD
- **Applies to:** reusable libraries
- **Directive:** Expose few robust entry points. Implement `From<T>` only when
  conversion is infallible, semantically lossless, value-preserving, and the
  single obvious conversion between the types. Use `TryFrom` when conversion can
  fail or validate, and use a named method when representation, byte order,
  rounding, normalization, policy, or domain interpretation should remain
  visible. Implement `From` rather than `Into` directly so the standard blanket
  implementation supplies both directions of the conversion vocabulary.
- **Why:** `From` is applied implicitly through blanket conversions and `?`, so a
  lossy or non-obvious implementation changes values at call sites its author
  never reviewed. A named method forces the caller to see the interpretation.
- **Exceptions:** Targeted internal libraries may expose expert controls when the
  domain and ownership contract remain clear. Information that is not
  semantically relevant, such as spare container capacity, need not be preserved.
- **Mechanical owner:** Round-trip or invariant tests where meaningful, error
  tests for fallible conversion, semver checks, and review.
- **Sources:** Preference R13, R14, R15, R98; standard-library `From`, `Into`,
  and `TryFrom` documentation.

### CORE-API-003 Implement capability traits only when truthful

- **Strength:** MUST
- **Applies to:** public and domain types
- **Directive:** Implement common traits when their contracts are meaningful for
  the type, not to satisfy a checklist. Make `Clone` preserve intended logical
  semantics and acknowledge material cost; implement `Default` only for a valid,
  unsurprising baseline; and treat `Serialize` and `Deserialize` as versioned
  data-format surfaces that preserve invariants, using feature gates or custom
  deserialization where needed. Let `Send` and `Sync` follow the type's actual
  thread-safety properties rather than replacing `Rc` with `Arc` solely to force
  them.
- **Why:** Each of these traits is a contract callers build on: `Default` invents
  a value the domain may not admit, `Clone` hides cost or aliasing, and
  `Serialize` freezes a durable data format. Deriving one to satisfy a checklist
  commits the type to semantics it does not actually have.
- **Exceptions:** A repository may deliberately require one of these
  capabilities, but the type design must then satisfy its real contract.
  Manually implementing `Send` or `Sync` is unsafe work and requires a written
  safety argument and unsafe review.
- **Mechanical owner:** Compile-time trait assertions where contractual,
  serialization compatibility and invariant tests, unsafe review, and public API
  review.
- **Sources:** Preference R175; Rust API Guidelines.

### CORE-API-004 Require a coherent capability from extension traits

- **Strength:** SHOULD
- **Applies to:** traits that add method-call syntax to types the crate does not
  own
- **Directive:** Introduce an extension trait only when its methods form one
  named, reusable capability and method syntax materially improves use or
  generic composition. Prefer inherent methods for owned types, free functions
  for isolated operations, and a newtype or wrapper when behavior needs distinct
  semantics, state, or invariants. Avoid miscellaneous `*Ext` buckets and broad
  blanket implementations whose supported receiver set is not intentional.
- **Why:** An extension trait adds methods to types the crate does not own, so
  its names compete with inherent methods and every other trait in scope. A
  miscellaneous bucket makes that collision surface unbounded and turns an
  unrelated dependency upgrade into an ambiguity error at the call site.
- **Public contract:** Document the capability, supported implementors, import
  path, ownership behavior, and whether downstream implementations are allowed.
  Seal the trait and say so when only the defining crate should implement it;
  otherwise treat implementor freedom, required items, supertraits, blanket
  implementations, and new methods as compatibility commitments. Review names
  against inherent methods and other likely traits in scope, because method
  lookup or ambiguity can change as dependencies evolve.
- **Exceptions:** A narrow, opt-in repository prelude may re-export a cohesive
  set of extension traits. Compatibility adapters may retain an older extension
  trait while steering new callers to a clearer API.
- **Mechanical owner:** Public consumer fixtures, documentation tests,
  semver checks where applicable, and API review. Compile representative calls
  both through method syntax and fully qualified trait syntax when collision or
  inference behavior matters.
- **Sources:** Preference R158, R179; Rust Reference trait-coherence rules, Rust
  Book method disambiguation guidance, Rust API Guidelines sealing guidance.

### CORE-API-005 Reserve deref coercion for transparent pointer behavior

- **Strength:** SHOULD
- **Applies to:** implementations of `Deref`, `DerefMut`, `AsRef`, `AsMut`,
  `Borrow`, and `BorrowMut`
- **Directive:** Implement `Deref` only when the wrapper transparently and
  cheaply behaves like a stable target and implicit coercion is unsurprising.
  Do not use it to simulate inheritance, expose a convenient field, or forward a
  domain newtype's methods. Prefer explicit domain methods or `AsRef` for a cheap
  reference conversion. Use `Borrow` only when the owned and borrowed forms have
  equivalent `Eq`, `Ord`, and `Hash` behavior.
- **Why:** Deref coercion is invisible at the call site and participates in
  method resolution, so a non-transparent implementation lets callers reach
  behavior they never asked for and lets a wrapper's invariant be edited through
  the target without passing any of its validation.
- **Mutable access:** Implement `DerefMut`, `AsMut`, or `BorrowMut` only when
  callers may safely perform every operation exposed through the mutable target.
  Do not expose mutable access that can violate the wrapper's invariant or bypass
  required validation.
- **Compatibility:** Treat the deref target and its method-resolution effects as
  public API. Review collisions between wrapper and target methods, and avoid an
  unexpectedly fallible or expensive dereference.
- **Exceptions:** Preserve an established pointer-like public contract when
  removal would be more harmful than continued support. Purpose-built guard,
  ownership, storage, and transparent collection/string wrappers may be
  pointer-like when they satisfy the full contract.
- **Mechanical owner:** API review, invariant tests, and downstream consumer
  fixtures that exercise coercion and method resolution.
- **Sources:** Preference R180; standard-library `Deref`, `DerefMut`, `AsRef`,
  and `Borrow` documentation; Rust API Guidelines C-DEREF.

### CORE-API-006 Add generic inputs for demonstrated flexibility

- **Strength:** SHOULD
- **Applies to:** public function and constructor parameters
- **Directive:** Use a concrete type or ordinary borrow when one representation
  is the actual contract. Add `impl Trait` or a named generic parameter when the
  bound itself expresses the required capability and multiple natural caller
  forms provide a demonstrated usability or composition benefit. Do not add
  `AsRef`, `Into`, or similar conversion bounds for hypothetical flexibility.
- **Why:** Every bound is a compatibility commitment and a monomorphization cost
  paid at each call site, spent in exchange for flexibility that speculative
  conversion bounds rarely deliver. Loosening a concrete parameter later is easy;
  removing an unused bound is a breaking change.
- **Conversion contracts:** Use `AsRef` for cheap borrowed conversion and `Into`
  only for an infallible consuming conversion; use `TryInto` or a named operation
  when conversion may fail or carries domain meaning. Make ownership, allocation,
  normalization, and validation at the conversion point visible in documentation
  and implementation.
- **Costs and compatibility:** Check inference and call-site clarity, accepted
  implementation breadth, code size, compile time, and optimization needs.
  Argument-position `impl Trait` is an anonymous generic parameter, and changing
  between it and a named parameter can break callers that specify generic
  arguments. Treat bounds and parameter form as public API commitments.
- **Exceptions:** Prefer a real capability bound such as `IntoIterator` when the
  implementation genuinely needs only that capability. A thin generic
  convenience function may normalize input and delegate to a concrete
  implementation to contain code generation.
- **Mechanical owner:** Documentation tests, representative external consumer
  fixtures, semver checks where required, and measured size or compile-time
  evidence when those costs drive the design.
- **Sources:** Preference R181; Rust API Guidelines C-GENERIC and
  C-CALLER-CONTROL; Rust Reference argument-position `impl Trait`; standard
  library `Into` documentation.

### CORE-API-007 Separate human formatting from machine encoding

- **Strength:** MUST
- **Applies to:** `Display`, `Debug`, `FromStr`, logs, persistence, and protocol
  representations
- **Directive:** Use `Display` for the type's single obvious human-facing form
  and `Debug` for programmer-facing diagnostics. Do not parse either output for
  program logic or treat it as a stable storage, interchange, or wire format
  unless the public contract explicitly says otherwise. Use dedicated,
  versioned serialization or protocol encoding APIs for machine representations.
- **Why:** Text written for people carries no compatibility guarantee, so a
  caller that parses it breaks on a wording change nobody considered breaking,
  and a `Debug` format that reaches a log carries whatever the type holds —
  including secrets — into storage the type never agreed to.
- **Parseable display:** When `Display` is intentionally lossless and
  machine-parseable, document its grammar and compatibility policy, make
  `FromStr` accept that form, and test `value.to_string().parse()` round trips.
  Use named display adapters when the type has multiple useful textual forms.
- **Diagnostics:** Assume derived and dependency `Debug` formats can change.
  Keep secrets and other sensitive fields out of both formatting surfaces.
  Return structured fields or variants when callers need machine-actionable
  information rather than requiring them to parse text.
- **Exceptions:** A standard or repository may designate one canonical textual
  machine format. That explicit choice makes its grammar, normalization, and
  compatibility part of the API and requires independent known-answer examples;
  self-round trips alone do not prove protocol conformance.
- **Mechanical owner:** Documentation and round-trip tests for declared textual
  contracts, known-answer or interoperability tests for protocol formats,
  secret-redaction tests where relevant, and review.
- **Sources:** Preference R18, R129, R182; standard-library `Display`, `Debug`,
  and `FromStr` documentation.

### CORE-API-008 Use `#[non_exhaustive]` only for intended source evolution

- **Strength:** SHOULD
- **Applies to:** public enums, structs, and enum variants
- **Directive:** Add `#[non_exhaustive]` when future variants or fields are an
  intended compatibility path, preferably when the public item is first
  introduced. Do not apply it mechanically to every public type. Keep genuinely
  closed domain sets exhaustive, and keep matches inside the defining crate
  meaningfully exhaustive so a new variant forces an explicit local decision.
- **Why:** The attribute buys source compatibility and nothing else. Applied
  reflexively it costs every downstream caller exhaustive matching and literal
  construction while proving nothing about unknown runtime values, and adding it
  to an existing item is itself the breaking change it was meant to avoid.
- **Compatibility boundary:** `#[non_exhaustive]` changes what downstream crates
  may construct and how they must match. Adding the attribute to an existing
  public item can therefore be a breaking source change. Adding a variant to a
  non-exhaustive enum may be source-compatible, but it can still change behavior
  and can affect layout-sensitive, FFI, serialized, or generated
  representations.
- **Runtime-data boundary:** `#[non_exhaustive]` does not accept, preserve, or
  interpret unknown runtime values. Protocols and durable formats that must
  retain unrecognized values need an explicit raw-preserving representation such
  as `Unknown(raw)`. Design and test serialization behavior explicitly rather
  than inferring it from the attribute.
- **Exceptions:** A closed public enum may intentionally require
  exhaustive downstream matches. Layout-sensitive or FFI types require the
  unsafe and FFI review paths in addition to source-compatibility analysis.
- **Mechanical owner:** Downstream consumer fixtures, semantic-version checks,
  meaningful exhaustive matches inside the defining crate, and
  unknown-wire-value tests.
- **Sources:** Preferences R89, R159, and R183; Rust Reference
  `non_exhaustive` documentation; Cargo SemVer Compatibility guide.

### CORE-API-009 Use `#[must_use]` as a targeted diagnostic

- **Strength:** SHOULD
- **Applies to:** value-returning types, functions, methods, and traits
- **Directive:** Add `#[must_use]` when discarding the value usually indicates a
  defect or an unfinished operation. Put it on a type when nearly every value of
  that type requires observation; put it on a function, method, or trait
  declaration when the obligation belongs to that operation. Include an
  actionable message when it explains the lost effect or next step better than
  the default diagnostic.
- **Why:** The attribute emits a suppressible lint, so relying on it as an
  enforcement mechanism puts the obligation somewhere it cannot be enforced.
  Applied broadly it also fails downstream builds that deny warnings, spending
  real consumer breakage on a diagnostic.
- **Noise control:** Do not annotate public APIs mechanically. Avoid an
  unqualified function-level annotation when its return type is already
  must-use unless a specific message materially improves the diagnostic. Treat
  Clippy's `must_use_candidate` as review input rather than an automatic rewrite,
  and heed `double_must_use` and `must_use_unit`.
- **Correctness boundary:** The attribute emits a suppressible lint; it is not a
  correctness, soundness, security, transaction, or resource-lifetime
  mechanism. A safe API must remain safe when its result is ignored. When
  discarding is intentional, make that intent explicit with `let _ = value` or
  `_ = value`; this syntax does not waive the repository's error-observability
  policy.
- **Compatibility:** Adding `#[must_use]` is generally a compatible lint change,
  but it can fail downstream builds that deny warnings. Consider representative
  consumers and release notes when adding it broadly to an established API.
  Place method policy on a trait declaration, not only on an implementation
  method where it has no effect.
- **Exceptions:** Cheap queries, ordinary returned data, internal helpers,
  generated code, and side-effecting operations may reasonably remain
  unannotated when intentional discards are common or the warning would not
  guide a correction.
- **Mechanical owner:** `unused_must_use`, curated Clippy
  `must_use_candidate`/`double_must_use`/`must_use_unit` policy, downstream
  consumer fixtures, and review.
- **Sources:** Preferences R176 and R184; Rust Reference `must_use`
  documentation; Cargo SemVer Compatibility guide; Clippy lint documentation.

### CORE-OWN-001 Transfer ownership before sharing it

- **Strength:** PREFER
- **Applies to:** buffers, pipelines, and cross-thread work
- **Directive:** Borrow for observation, move owned values when the callee or next
  stage owns them, and introduce `Arc` or locks only for real concurrent sharing.
  Remember that moving a `Vec<T>` does not copy its allocation.
- **Why:** `Arc` and locks convert a question the compiler answers into one the
  runtime answers, adding contention, reference-counting traffic, and a class of
  deadlock that a move makes impossible. Sharing is also hard to retract once
  callers depend on it.
- **Exceptions:** Shared immutable data or ownership topology may make `Arc`
  clearest; measure hot-path reference counting.
- **Mechanical owner:** Benchmarks, allocation tests, review.
- **Sources:** Preference R16, R17, R99, R101, R162.

### CORE-OWN-003 Match borrows to the capability actually required

- **Strength:** SHOULD
- **Applies to:** function and method signatures
- **Directive:** Request the narrowest meaningful capability and retain it only
  as long as needed: prefer shared over mutable access, borrow the deepest useful
  referent, and accept slices or string slices when container ownership or
  capacity is irrelevant. Extract a field-level helper when a whole-object
  receiver overstates mutation, blocks independent field access, or creates real
  borrow coupling.
- **Why:** An overstated receiver is contagious. Taking `&mut self` to read one
  field blocks every other field access for the borrow's duration, so callers
  restructure their own code around a capability the operation never needed.
- **Exceptions:** Keep a whole-object receiver when the operation preserves an
  invariant spanning fields, representation hiding matters, a trait or compatible
  public signature requires it, or extraction would make clear code harder to
  follow. Do not refactor solely to shorten a borrow cosmetically or support an
  unmeasured compile-time claim.
- **Mechanical owner:** Compilation, API and compatibility tests, and review.
- **Sources:** Preference R167, R177; Let's Get Rusty advisory source.

### CORE-ERR-001 Return structured, actionable errors

- **Strength:** MUST
- **Applies to:** production libraries
- **Directive:** Return typed errors that preserve sources and machine-relevant
  context. Distinguish incomplete, malformed, retryable, overload, and shutdown
  conditions when callers act differently.
- **Why:** A caller that cannot tell retryable from malformed either retries a
  permanent failure forever or abandons a recoverable one. Collapsing the
  distinction into a message moves that decision to string matching, which
  breaks silently the next time the wording changes.
- **Exceptions:** Applications may add opaque report context at presentation
  boundaries.
- **Mechanical owner:** Public API tests and review.
- **Sources:** Preference R21-R24, R87.

### CORE-ERR-002 Keep panic exceptional

- **Strength:** MUST NOT
- **Applies to:** production libraries and hostile-input paths
- **Directive:** Do not use panic, `unwrap`, or `expect` for reachable operational
  failure. A panic is acceptable only for an internal invariant whose violation
  makes safe continuation impossible and whose proof is documented and tested.
- **Why:** A panic on a reachable path converts a failure the caller could have
  handled into a lost thread, a poisoned lock, or a downed process. On a path
  that hostile input can reach, that is an availability attack with a one-line
  exploit.
- **Exceptions:** Focused tests and intrinsically infallible tiny examples may use
  them without modeling a production panic path.
- **Mechanical owner:** Clippy, fuzzing, tests, review.
- **Sources:** Preference R25-R27, R54, R141.

### CORE-ERR-003 Make intentional error loss explicit and observable

- **Strength:** MUST
- **Applies to:** fallible input processing, iteration, streams, tasks, and
  message delivery
- **Directive:** Propagate or handle errors by default. Do not erase them through
  `Result::ok`, `filter_map`, `flatten`, ignored return values, or equivalent
  shorthand unless skipping failures is an explicit documented policy. Name the
  best-effort behavior and expose suitable aggregate counts, diagnostics,
  quarantine output, or another repository-appropriate signal. Preserve
  distinctions such as incomplete, malformed, retryable, overload, and shutdown
  when they change caller behavior.
- **Why:** A discarded error produces the same successful-looking result whether
  the input was clean or half of it failed, so the defect surfaces as missing
  data long after the run that caused it and with nothing left to diagnose.
- **Exceptions:** A deliberately lossy probe, cache, sampling path, or
  best-effort importer may discard individual failures when the consequence and
  observability policy are explicit. Avoid per-item logging in hot paths; report
  at a bounded aggregation boundary instead.
- **Mechanical owner:** `must_use` and Clippy where applicable, structured
  metrics or reports, semantic error-policy tests, and review.
- **Sources:** Preference R176.

### CORE-STYLE-004 Make ownership operations locally obvious

- **Strength:** SHOULD
- **Applies to:** mutation, shadowing, and cloning
- **Directive:** Narrow mutable scopes. Shadow only for a clear transformation.
  Use `Arc::clone`/`Rc::clone` for shared ownership and reconsider a clone added
  only to satisfy the borrow checker.
- **Why:** A clone added to quiet the borrow checker records no intent, so a
  later reader cannot tell whether the copy is semantically required or
  accidental — and it survives every refactor that would have removed the
  conflict that produced it.
- **Exceptions:** A measured specialization may choose a different ownership path
  with a documented contract.
- **Mechanical owner:** Review and benchmarks.
- **Sources:** Preference R155, R162.
