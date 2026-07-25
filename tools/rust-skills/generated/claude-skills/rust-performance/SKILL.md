---
name: rust-performance
description: Measure and optimize Rust latency, throughput, allocation, memory, cache behavior, branches, vectorization, SIMD, code size, and compile time. Use for performance claims or performance-sensitive paths. Do not add complexity for hypothetical gains without a representative measurement.
---

# Engineer Rust Performance

## Define the claim

1. Read `$rust-core`, repository budgets, supported hardware and features,
   existing profiles, correctness tests, and the actual call path.
2. Read [performance workflow](references/performance.md).
3. Record whether the path is measured, workload size and distribution,
   execution frequency, latency and throughput goals, allocation and memory
   behavior, cache and branch implications, vectorization, code-size impact, and
   compile-time impact.

## Measure before changing

Preserve a correct readable reference implementation when optimization obscures
reasoning. Use a benchmark whose every iteration performs the same declared
workload. Record target, CPU, toolchain, features, build profile, samples, and
noise controls. Measure the shipped artifact for size work.

Consider algorithm and data movement before unsafe, SIMD, parallelism, or
micro-optimization. Route harness correctness to `$rust-testing`, parallel
ownership to `$rust-async-concurrency`, unsafe optimization to
`$rust-unsafe-ffi`, and domain equivalence to its owner.

## Accept a change only with evidence

Normally require before-and-after measurements, preserved correctness tests,
tradeoff analysis, and a reproduction command. Reject clear-code regressions for
hypothetical gains. State when a platform-specific path retains a correct
fallback.

## Output

Provide the measurement brief, raw comparison, uncertainty, correctness
evidence, complexity and portability cost, code/compile-size impact, and
recommendation.
