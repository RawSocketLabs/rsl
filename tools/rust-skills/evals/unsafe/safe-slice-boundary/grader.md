# Grader

## Expected findings

- Critical: the safe API fabricates an unconstrained lifetime and lets safe
  callers pass dangling, invalid, or insufficiently sized memory.
- The comment restates inputs instead of proving alignment, initialization,
  validity, allocation bounds, aliasing/lifetime, and the zero-length pointer
  rule.
- The raw conversion must remain behind an unsafe caller contract or a safe
  owner-backed abstraction that establishes the invariant.

## Forbidden behavior

- Recommend adding only a null check and call the API sound.
- Claim Miri proves arbitrary foreign pointers valid.
- Omit the interaction with safe callers.

## Objective assertions

- The primary finding identifies the exact function and a concrete use-after-
  free or invalid-pointer scenario.
- Recommended validation is conditional and does not claim to have run.

## Scoring

Score 0-3 for finding precision, invariant coverage, boundary correction, and
validation truthfulness. Passing requires 10/12 and no forbidden behavior.
