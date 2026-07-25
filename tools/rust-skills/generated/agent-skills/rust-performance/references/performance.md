# Performance Workflow

### CORE-OWN-002 Make steady-state allocation a declared choice

- **Strength:** MUST
- **Applies to:** declared DSP and sustained hot loops
- **Directive:** Avoid steady-state allocation after initialization where the
  repository declares an allocation-sensitive path. Prefer owned reusable
  buffers and bounded recycling with explicit overload behavior.
- **Why:** Allocation in a sustained loop introduces a variable-latency call and
  an unbounded failure mode into a path whose whole purpose is bounded, uniform
  work. The cost shows up as tail latency and fragmentation under load, not in
  the average measured on an idle machine.
- **Exceptions:** Initialization and control-plane allocation are normal; accept
  a measured allocation when it keeps the design safer or clearer.
- **Mechanical owner:** Allocation benchmarks and profiles.
- **Sources:** Preference R36, R37, R99.

### CORE-PERF-001 Measure before accepting performance complexity

- **Strength:** MUST
- **Applies to:** performance-motivated design, unsafe, SIMD, and parallelism
- **Directive:** Define workload and metric, establish a scalar/correctness
  reference and before/after baseline, profile the bottleneck, then measure the
  specialized implementation locally. Record hardware, toolchain, features, and
  numerical contract.
- **Why:** Intuition about bottlenecks is wrong often enough that unmeasured
  optimization usually buys complexity, unsafe code, or a numerical change in a
  path that was never hot. Without a preserved reference and a recorded baseline
  there is also nothing left to prove the result is still correct or faster.
- **Exceptions:** Experimental code may instrument first, but may not claim an
  improvement without evidence.
- **Mechanical owner:** Criterion or repository benchmark harness, flamegraphs,
  allocation tools, controlled CI where stable.
- **Sources:** Preference R37-R43, R47, R110, R133, and R206; CodeAesthetic
  advisory source.

### CORE-PERF-002 Measure the shipped artifact before optimizing binary size

- **Strength:** MUST
- **Applies to:** changes motivated by executable, firmware, WebAssembly module,
  package, or container-image size
- **Directive:** Define the artifact boundary, target, toolchain, features, and
  size metric; record reproducible before/after bytes; preserve correctness; and
  measure each stable profile change independently. Treat stripping, `opt-level`
  choices, LTO, and codegen-unit changes as tradeoffs rather than guaranteed
  wins. Require explicit approval for behavior- or support-changing steps such
  as panic abort, removed diagnostics, nightly `build-std`, `no_std`, `no_main`,
  dynamic linking, binary packing, or additional unsafe code.
- **Why:** Size lives in the shipped artifact, so a measurement taken anywhere
  else — an unstripped debug build, a library rather than the binary — bears no
  fixed relationship to it. Bundling profile changes hides which one paid, and
  several of the largest reductions quietly remove panic messages or diagnostics.
- **Exceptions:** A repository with no material size objective need not add a
  size workflow. Investigation may use locally available inspection tools, but
  unavailable tools are reported rather than counted as passing.
- **Mechanical owner:** Repository release profile, artifact-size script or CI,
  and target-specific inspection tools such as `cargo-bloat`,
  `cargo-llvm-lines`, or Twiggy.
- **Sources:** Preference R172; Cargo profile documentation; `min-sized-rust`.

### PERF-MEASURE-001 Define the workload before optimizing

- **Strength:** MUST
- **Applies to:** latency, throughput, allocation, memory, cache, branch, SIMD,
  parallelism, binary-size, and compile-time claims
- **Directive:** Record workload size and distribution, execution frequency,
  named timing endpoints, latency and throughput requirements, allocation and
  memory behavior, cache and branch implications, vectorization opportunity,
  code-size impact, compile-time impact, target hardware, toolchain, features,
  build profile, and measurement method before accepting complexity.
- **Why:** A faster microbenchmark can optimize irrelevant work, hide setup,
  change semantics, or regress the shipped workload.
- **Exceptions:** A clear asymptotic correction or removal of proven redundant
  work may precede a full benchmark when correctness remains independently
  tested and the missing measurement is reported.
- **Mechanical owner:** Reproducible benchmark or artifact command, recorded
  environment, preserved correctness tests, and review.
- **Sources:** Preferences R35-R40, R111-R116, and R172-R174.

### PERF-REF-001 Preserve a readable correctness oracle

- **Strength:** SHOULD
- **Applies to:** difficult SIMD, unsafe, parallel, branchless, table-driven,
  generated, and bit-manipulation optimizations
- **Directive:** Retain a readable scalar or specification-shaped reference
  implementation or an equivalent independent oracle when optimized code is
  difficult to verify. Run shared conformance tests across both paths and keep a
  correct fallback for supported targets without the optimization.
- **Why:** A local speedup is not valuable if reviewers cannot establish
  equivalence or unsupported targets lose correctness.
- **Exceptions:** A small, directly proven transformation may use focused
  expected-value tests instead of a production reference path.
- **Mechanical owner:** Shared conformance tests, target-feature tests,
  differential properties, and benchmark comparison.
- **Sources:** Preferences R36, R38-R40, R112, and R118.

### PERF-BENCH-001 Keep every timed iteration representative

- **Strength:** MUST
- **Applies to:** benchmarks with mutable, consumable, cached, stateful, pooled,
  or reusable inputs
- **Directive:** Ensure every measured iteration performs the same declared
  workload. Restore consumed or mutated state outside the timed region or
  construct representative state without timing setup. Detect warmup, cache,
  batching, and optimizer effects deliberately.
- **Why:** A benchmark that measures full work once and reduced or invalid work
  later cannot support a performance claim.
- **Exceptions:** A benchmark explicitly measuring steady-state evolution may
  retain state when that evolving workload is the documented subject and every
  sample remains interpretable.
- **Mechanical owner:** Harness inspection, iteration-state assertions,
  independent measurement, and benchmark smoke tests.
- **Sources:** Preferences R118 and R174.

### PERF-SIZE-001 Measure the shipped binary configuration

- **Strength:** MUST
- **Applies to:** binary size, firmware footprint, feature pruning, LTO, panic
  strategy, allocator, stripping, and code-generation claims
- **Directive:** Measure the actual shipped target, profile, features, linker,
  stripping, LTO, panic strategy, and artifact. Separate code, data, debug, and
  packaging effects when relevant. Treat behavior-changing size techniques as
  repository decisions.
- **Why:** Development artifacts and host builds rarely represent the deployed
  binary.
- **Exceptions:** Early trend checks may use a proxy configuration when it is
  labeled and followed by the release artifact before acceptance.
- **Mechanical owner:** Reproducible artifact build, size report, behavior
  tests, and target validation.
- **Sources:** Preference R172 and `min-sized-rust` advisory guidance.
