---
name: rust-testing
description: Select, implement, and assess Rust unit, integration, documentation, property, fuzz, snapshot, concurrency, unsafe, protocol, benchmark, and verification evidence. Use whenever behavior changes or claims need proof. Do not run every tool mechanically when the repository or risk makes it inapplicable.
---

# Test and Verify Rust

## Select evidence by risk

1. Read `$rust-core`, the repository validation contract, CI, existing tests,
   features, targets, fixtures, and affected behavior.
2. Name the property at risk before selecting a tool. Read
   [verification guidance](references/verification.md) for evidence layers,
   deterministic tests, regressions, adversarial inputs, and benchmark validity.
   Read [testing techniques](references/techniques.md) for property testing,
   fuzzing, and specialized-tool applicability.
3. Activate the domain owner: `$rust-protocol` for wire vectors, `$rust-dsp` for
   numeric contracts, `$rust-async-concurrency` for concurrency schedules,
   `$rust-unsafe-ffi` for unsafe proof, `$rust-performance` for workloads, and
   `$rust-embedded` for hardware behavior.

## Build the smallest sufficient ladder

- Use focused unit tests for local semantics and integration tests for public
  contracts and component interaction.
- Compile doctests and examples that teach supported usage.
- Use property tests for broad algebraic or stateful invariants, fuzzing for
  hostile parser surfaces, snapshots for deliberately reviewed stable output,
  Loom for tractable concurrency models, Miri or sanitizers for applicable
  unsafe behavior, and mutation testing when it adds evidence.
- Treat protocol round trips as necessary but insufficient; add independent
  known-answer or interoperability vectors.
- Keep fixtures attributed, bounded, deterministic where possible, and clear
  about synthetic versus captured provenance.

## Report truthfully

Run repository-required checks and risk-specific checks that are safe and
available. Record passed, failed, skipped, unavailable, and inapplicable
separately. Never convert missing tools into success or confuse compilation,
native runtime, correctness, coverage, and performance evidence.

## Output

Provide the risk-to-test map, changed tests or fixtures, exact commands and
results, limitations, and follow-up evidence requiring CI, hardware, time, or
external implementations.
