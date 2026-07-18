# Verification

Select evidence for the property that can fail. Do not run every tool for every
change, and do not let one green test layer stand in for a different risk.

### CORE-TEST-001 Match evidence to risk

- **Strength:** MUST
- **Applies to:** production changes
- **Directive:** Combine unit, public integration, doc, property, fuzz,
  reference-vector, concurrency, platform, and performance evidence as the
  affected contract requires.
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
- **Exceptions:** Record why an automated reproducer cannot be retained.
- **Mechanical owner:** Tests and corpus checks.
- **Sources:** Preference R130, R131.

### CORE-TEST-004 Test adversarial and bounded behavior

- **Strength:** MUST
- **Applies to:** parsers, unsafe boundaries, queues, lifecycle, and resource caps
- **Directive:** Fuzz critical hostile surfaces; validate lengths before indexing
  or allocation; exercise below/at/above limits, overload, backpressure,
  cancellation, and shutdown. Minimize and retain discovered failures.
- **Exceptions:** None for memory-safety boundaries.
- **Mechanical owner:** Fuzzing, property tests, sanitizers, lifecycle tests.
- **Sources:** Preference R54, R58, R93, R134, R137.

### CORE-TEST-005 Distinguish build, runtime, and performance evidence

- **Strength:** MUST
- **Applies to:** platform-specific and optimized code
- **Directive:** Treat cross-compilation as buildability only. Run native
  correctness on first-class targets and architecture-specific paths on
  representative hardware. Keep wall-clock assertions out of functional tests.
- **Exceptions:** Document unavailable hardware and release risk.
- **Mechanical owner:** Native CI, benchmarks, profiles.
- **Sources:** Preference R38, R44, R50, R133, R135.

### CORE-PERF-001 Measure before accepting performance complexity

- **Strength:** MUST
- **Applies to:** performance-motivated design, unsafe, SIMD, and parallelism
- **Directive:** Define workload and metric, establish a scalar/correctness
  reference and before/after baseline, profile the bottleneck, then measure the
  specialized implementation locally. Record hardware, toolchain, features, and
  numerical contract.
- **Exceptions:** A prototype may instrument first, but may not claim an
  improvement without evidence.
- **Mechanical owner:** Criterion or repository benchmark harness, flamegraphs,
  allocation tools, controlled CI where stable.
- **Sources:** Preference R37-R43, R47, R110, R133.

### Completion report

State the commands and target environments actually observed, their results, and
the material checks not run. Review the final diff after verification; do not
present recommended commands as completed evidence.
