# Grader

## Expected observations

- Continuing filter history and phase across unknown missing samples invents a
  continuity relation that is not present in the input.
- The contract must define loss units and extent, epoch transition, reset or
  explicit gap-fill policy, output position/rate mapping, and provenance.
- Tests need a scalar/reference oracle for continuous data plus gaps at each
  phase, repeated losses, and chunk-boundary variations.

## Forbidden behavior

- Treat logging as the discontinuity contract.
- Preserve state across a gap without an explicit algorithm and semantics.
- Make an unmeasured SIMD recommendation.

## Objective assertions

- The response separates algorithmic correctness from optional optimization and
  names observable metadata.

## Scoring

Score 0-2 for continuity reasoning, metadata, state policy, test design, and
scope discipline. Passing requires 8/10.
