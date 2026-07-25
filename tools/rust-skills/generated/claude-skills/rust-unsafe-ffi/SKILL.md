---
name: rust-unsafe-ffi
description: Design and review unsafe Rust, safe wrappers, manual Send or Sync, ABI boundaries, foreign ownership, callbacks, and FFI build integration. Use whenever unsafe code or a foreign interface is introduced, changed, or relied upon. Do not use unsafe merely to bypass borrow checking or for unmeasured optimization.
---

# Review Unsafe Rust and FFI

## Establish permission and necessity

1. Read `$rust-core`, repository unsafe policy, target ABIs, panic strategy,
   foreign headers and versions, build scripts, ownership conventions, and
   existing safety documentation.
2. Confirm unsafe or FFI is allowed and that a safe or established crate cannot
   satisfy the requirement adequately.
3. Read [unsafe and FFI review](references/unsafe-and-ffi.md).

## Write the safety argument first

State the invariant, who establishes it, who preserves it, and how lifetime,
aliasing, initialization, alignment, provenance, concurrency, panic/unwind,
drop, and safe-caller interaction uphold it. For FFI also state ABI, layout,
calling convention, nullability, lengths, ownership transfer, thread affinity,
callback lifetime, error mapping, and foreign reentrancy.

Contain unsafe in the smallest auditable module and expose a safe API whose
invariant callers cannot violate without unsafe code. Make every unsafe block
justify the local obligation it discharges. Treat manual `Send` or `Sync` as a
whole-type proof.

## Verify

Route tests to `$rust-testing`, concurrency to `$rust-async-concurrency`,
performance motivation to `$rust-performance`, embedded ABI work to
`$rust-embedded`, and dependency/build surfaces to
`$rust-dependencies-security`. Use Miri, sanitizers, target ABI tests,
compile-fail tests, foreign integration tests, and fuzzing where feasible.

## Output

Provide the written safety case, exposed safe contract, ABI and ownership
analysis, validation actually run, unavailable evidence, and residual risk.
