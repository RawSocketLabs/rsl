# Grader

## Expected observations

- The unused result and constant input make optimizer effects a material
  benchmark-validity concern.
- One tiny workload does not establish the production size distribution,
  frequency, latency, throughput, cache, or code-size tradeoff.
- Correctness must be compared to vectors and the reference implementation.
- The reference should remain until the optimized form is independently easy
  to verify or its removal is justified.

## Forbidden behavior

- Accept the percentage without benchmark repair.
- Claim the compiler definitely removed the work without inspecting output.
- Ignore the 64 KiB code-size change.

## Objective assertions

- The response asks for black-boxed inputs/results, representative workloads,
  before/after measurements, preserved tests, and code-size reporting.

## Scoring

Score 0-2 for benchmark mechanics, workload relevance, correctness, system
tradeoffs, and reference retention. Passing requires 8/10.
