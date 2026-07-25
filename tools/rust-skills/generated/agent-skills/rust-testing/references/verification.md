# Verification

Select evidence for the property that can fail. Do not run every tool for every
change, and do not let one green test layer stand in for a different risk.

### CORE-EXAMPLE-002 Compile real, production-shaped examples

- **Strength:** MUST
- **Applies to:** substantial rustdoc and examples
- **Directive:** Use the supported public API, `Result` and `?` for normal failure,
  real feature gates, and transparent compiled source. Prefer execution in CI;
  reserve `no_run` and `ignore` for documented environmental constraints.
- **Why:** An example hidden behind `ignore`, or written against something other
  than the supported API, stops being evidence. It keeps passing review long
  after the interface it teaches has changed, and readers copy it anyway.
- **Exceptions:** Omit orthogonal setup explicitly; never replace essential
  behavior with a `TODO`.
- **Mechanical owner:** Doctests, `cargo check --examples`, feature CI.
- **Sources:** Preference R140, R141, R145-R147.

### CORE-TEST-001 Match evidence to risk

- **Strength:** MUST
- **Applies to:** production changes
- **Directive:** Combine unit, public integration, doc, property, fuzz,
  reference-vector, concurrency, platform, and performance evidence as the
  affected contract requires.
- **Why:** Each layer proves a different thing. A green unit suite says nothing
  about a hostile parser input, a scheduling race, or a target-specific path, so
  uniform effort buys confidence in the tiers that were never going to fail.
- **Exceptions:** Omit irrelevant tiers and report material evidence unavailable
  in the current environment.
- **Mechanical owner:** Repository test tiers and CI.
- **Sources:** Preference R51, R127.

### CORE-TEST-002 Keep tests deterministic and semantic

- **Strength:** MUST
- **Applies to:** automated tests
- **Directive:** Control seeds, clocks, and scheduling; report reproducers; treat
  flakiness as a defect; assert structured domain behavior rather than incidental
  display text or layout.
- **Why:** A flaky test teaches the team to rerun instead of investigate, which
  is how a genuine intermittent defect gets retried into a release. Asserting
  incidental text makes a formatting change look like a behavioral break, which
  costs the same attention in the opposite direction.
- **Exceptions:** Deliberate soak tests may explore nondeterminism but must record
  diagnostic context.
- **Mechanical owner:** Test harness and review.
- **Sources:** Preference R57-R59, R129.

### CORE-TEST-003 Preserve regressions and conformance

- **Strength:** MUST
- **Applies to:** corrected defects and interchangeable implementations
- **Directive:** Retain a minimized deterministic regression for a reproducible
  defect. Run one shared behavioral suite across scalar/optimized, codec, or
  backend implementations that promise the same contract.
- **Why:** A defect without a retained reproducer is fixed only until the next
  refactor reintroduces it. Implementations that promise one contract but are
  tested separately diverge in precisely the cases neither suite covers.
- **Exceptions:** Record why an automated reproducer cannot be retained.
- **Mechanical owner:** Tests and corpus checks.
- **Sources:** Preference R130, R131.

### CORE-TEST-004 Test adversarial and bounded behavior

- **Strength:** MUST
- **Applies to:** parsers, unsafe boundaries, queues, lifecycle, and resource caps
- **Directive:** Fuzz critical hostile surfaces; validate lengths before indexing
  or allocation; exercise below/at/above limits, overload, backpressure,
  cancellation, and shutdown. Minimize and retain discovered failures.
- **Why:** Hostile input reaches paths ordinary tests never construct, and the
  boundary cases — at the limit, one past it, under overload, mid-cancellation —
  are exactly where length checks, allocation caps, and shutdown logic fail.
- **Exceptions:** None for memory-safety boundaries.
- **Mechanical owner:** Fuzzing, property tests, sanitizers, lifecycle tests.
- **Sources:** Preference R54, R58, R93, R134, R137.

### CORE-TEST-005 Distinguish build, runtime, and performance evidence

- **Strength:** MUST
- **Applies to:** platform-specific and optimized code
- **Directive:** Treat cross-compilation as buildability only. Run native
  correctness on first-class targets and architecture-specific paths on
  representative hardware. Keep wall-clock assertions out of functional tests.
- **Why:** A successful cross-compile proves the code type-checks for a target,
  not that it runs correctly on one. Reporting it as passing conceals every
  endianness, alignment, and intrinsic-availability failure until the hardware
  is already in someone's hands.
- **Exceptions:** Document unavailable hardware and release risk.
- **Mechanical owner:** Native CI, benchmarks, profiles.
- **Sources:** Preference R38, R44, R50, R133, R135.

### CORE-TEST-006 Keep benchmark iterations representative

- **Strength:** MUST
- **Applies to:** benchmarks used to support a performance claim or
  performance-motivated change
- **Directive:** Ensure every measured sample exercises the declared workload.
  Regenerate or reset mutating and consuming inputs outside the timed operation,
  use an appropriate batching/setup API, keep setup and destruction outside the
  measurement unless they are part of the metric, and verify representative
  inputs and outputs. Treat a harness that measures changed state after its
  first iteration as invalid evidence and block the claim until the harness or
  replacement evidence is corrected.
- **Why:** A harness that consumes or mutates its input measures the declared
  workload once and something else thereafter — an emptied queue, an
  already-sorted vector. The number it reports is real and reproducible and
  describes a workload that never runs in production.
- **Exceptions:** A cumulative or stateful benchmark may intentionally evolve
  state only when that evolution is the documented production workload.
  End-to-end metrics may include construction, teardown, or I/O when the named
  metric includes those costs.
- **Mechanical owner:** Benchmark harness tests, review, and the selected
  measurement tool's setup or batching facilities.
- **Sources:** Preference R174; Criterion.rs timing-loop documentation.

### Completion report

State the commands and target environments actually observed, their results, and
the material checks not run. Review the final diff after verification; do not
present recommended commands as completed evidence.
