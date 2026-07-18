# API, Ownership, and Errors

### CORE-API-001 Preserve important invariants in types

- **Strength:** SHOULD
- **Applies to:** public and domain APIs
- **Directive:** Use a domain type when confusing values creates a meaningful
  defect. Prefer builders with validation enabled by default and granular,
  explicit opt-outs for protocol-invalid construction.
- **Exceptions:** Keep a primitive when the distinction is local and conversion
  noise would dominate.
- **Mechanical owner:** Tests and review.
- **Sources:** Preference R11, R12, R91.

### CORE-API-002 Keep public surfaces small and conventional

- **Strength:** SHOULD
- **Applies to:** reusable libraries
- **Directive:** Expose few robust entry points. Use conventional `From` only for
  infallible, unsurprising conversion; use `TryFrom` or named methods otherwise.
- **Exceptions:** Targeted internal libraries may expose expert controls when the
  domain and ownership contract remain clear.
- **Mechanical owner:** Semver checks and review.
- **Sources:** Preference R13, R14, R15, R98.

### CORE-OWN-001 Transfer ownership before sharing it

- **Strength:** PREFER
- **Applies to:** buffers, pipelines, and cross-thread work
- **Directive:** Borrow for observation, move owned values when the callee or next
  stage owns them, and introduce `Arc` or locks only for real concurrent sharing.
  Remember that moving a `Vec<T>` does not copy its allocation.
- **Exceptions:** Shared immutable data or ownership topology may make `Arc`
  clearest; measure hot-path reference counting.
- **Mechanical owner:** Benchmarks, allocation tests, review.
- **Sources:** Preference R16, R17, R99, R101, R162.

### CORE-OWN-002 Make steady-state allocation a declared choice

- **Strength:** MUST
- **Applies to:** declared DSP and sustained hot loops
- **Directive:** Avoid steady-state allocation after initialization where the
  repository declares an allocation-sensitive path. Prefer owned reusable
  buffers and bounded recycling with explicit overload behavior.
- **Exceptions:** Initialization and control-plane allocation are normal; accept
  a measured allocation when it keeps the design safer or clearer.
- **Mechanical owner:** Allocation benchmarks and profiles.
- **Sources:** Preference R36, R37, R99.

### CORE-ERR-001 Return structured, actionable errors

- **Strength:** MUST
- **Applies to:** production libraries
- **Directive:** Return typed errors that preserve sources and machine-relevant
  context. Distinguish incomplete, malformed, retryable, overload, and shutdown
  conditions when callers act differently.
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
- **Exceptions:** Focused tests and intrinsically infallible tiny examples may use
  them without modeling a production panic path.
- **Mechanical owner:** Clippy, fuzzing, tests, review.
- **Sources:** Preference R25-R27, R54, R141.

### CORE-SAFE-001 Contain unsafe behind a safe contract

- **Strength:** MUST
- **Applies to:** unsafe Rust and FFI
- **Directive:** Deny unsafe by default. When explicitly permitted, minimize the
  block, state invariants in an adjacent `SAFETY` explanation, provide a safe
  wrapper, and keep unwind, ownership, aliasing, and lifetime rules explicit.
- **Exceptions:** Raw binding crates may expose unsafe surfaces with complete
  safety documentation and scoped policy.
- **Mechanical owner:** Lints, Miri/sanitizers/fuzz/platform tests, review.
- **Sources:** Preference R45-R49, R165; Rustonomicon.
