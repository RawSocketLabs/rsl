# Unsafe Rust and FFI Review

### CORE-SAFE-001 Contain unsafe behind a safe contract

- **Strength:** MUST
- **Applies to:** unsafe Rust and FFI
- **Directive:** Deny unsafe by default. When explicitly permitted, minimize the
  block, state invariants in an adjacent `SAFETY` explanation, provide a safe
  wrapper, and keep unwind, ownership, aliasing, and lifetime rules explicit.
- **Why:** Unsafe code's obligations extend past the block to everything that can
  reach the same memory, so an unsafe surface exposed to safe callers makes
  soundness depend on every future caller reading the documentation. Containment
  is what keeps the audit finite and the proof reviewable.
- **Exceptions:** Raw binding crates may expose unsafe surfaces with complete
  safety documentation and scoped policy.
- **Mechanical owner:** Lints, Miri/sanitizers/fuzz/platform tests, review.
- **Sources:** Preference R45-R49, R165; Rustonomicon.

### UNSAFE-PROOF-001 Require a complete written safety argument

- **Strength:** MUST
- **Applies to:** every unsafe block or function, unsafe trait implementation,
  raw pointer, union, manual layout, manual `Send` or `Sync`, and safe wrapper
- **Directive:** State the invariant, who establishes it, who preserves it, and
  how lifetime, aliasing, initialization, alignment, provenance, concurrency,
  panic or unwind, drop, and interaction with safe callers satisfy it. Explain
  the local proof obligation at each unsafe block rather than paraphrasing the
  operation.
- **Why:** Unsafe correctness depends on obligations the compiler cannot check;
  an incomplete argument leaves safe callers able to trigger undefined
  behavior.
- **Exceptions:** None for reachable unsafe code. Generated bindings may
  centralize repeated ABI facts when generation provenance and wrapper
  invariants remain reviewable.
- **Mechanical owner:** Human/agent proof review, compile-fail tests, Miri and
  sanitizers when feasible, and targeted runtime tests.
- **Sources:** Preferences R42-R47, R69, R80-R82, R175, and the Rustonomicon.

### UNSAFE-BOUNDARY-001 Keep safe callers unable to violate the invariant

- **Strength:** MUST
- **Applies to:** safe wrappers, constructors, getters, mutation, callbacks,
  handles, buffers, and lifetime-carrying APIs around unsafe internals
- **Directive:** Validate or encode every unsafe prerequisite before entering
  the safe abstraction. Do not expose safe mutation, aliasing, ownership,
  lifetime, thread, initialization, or layout operations that can invalidate the
  proof. Make unsafe preconditions explicit only on genuinely unsafe caller
  surfaces.
- **Why:** A safe API that requires undocumented caller discipline is unsound
  even when its internal unsafe block is locally correct.
- **Exceptions:** A deliberately unsafe API may delegate a precise documented
  obligation to its unsafe caller.
- **Mechanical owner:** Safe-API adversarial tests, compile-fail tests, Miri,
  concurrency models, and review.
- **Sources:** Preferences R42-R47, R69, and R173.

### FFI-BOUNDARY-001 Specify the complete foreign contract

- **Strength:** MUST
- **Applies to:** `extern` functions, exported symbols, bindgen output, C/C++
  shims, callbacks, native handles, shared memory, and foreign build integration
- **Directive:** Record ABI and calling convention, symbol and library version,
  layout and integer widths, nullability, pointer-length relationships,
  ownership transfer, allocation and deallocation pairing, callback lifetime,
  thread affinity, reentrancy, synchronization, error mapping, panic/unwind,
  initialization, shutdown, and foreign resource drop behavior. Contain raw
  bindings behind a narrow safe wrapper.
- **Why:** ABI-compatible signatures can still be incorrect through ownership,
  lifetime, thread, or unwind mismatches.
- **Exceptions:** A platform SDK may own part of the contract when its exact
  version is pinned and the wrapper cites the relevant guarantee.
- **Mechanical owner:** Header/binding drift check, target ABI build and tests,
  foreign integration tests, sanitizers, and review.
- **Sources:** Preferences R80-R82 and the Rustonomicon.

### FFI-PANIC-001 Prevent unwinding across unsupported boundaries

- **Strength:** MUST
- **Applies to:** Rust callbacks called by foreign code and Rust exports called
  through ABIs that do not permit unwinding
- **Directive:** Keep panic from crossing an ABI boundary that does not
  explicitly support it. Choose repository policy for abort, catch-and-map, or
  impossible-panic proof, and ensure cleanup and foreign state remain valid.
- **Why:** Unwinding through an incompatible foreign stack is undefined or
  platform-dependent and may skip required cleanup.
- **Exceptions:** An explicitly unwind-capable ABI may permit propagation only
  when both sides and all intervening code support the contract.
- **Mechanical owner:** Panic-path tests, ABI review, and target integration.
- **Sources:** Preferences R21-R22, R81, and the Rustonomicon.
