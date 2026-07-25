# Grader

## Expected observations

- Dropping the in-progress future can discard the local length or payload bytes
  while the socket stream has advanced.
- Partial framing state should be owned by a persistent connection decoder, or
  cancellation should occur only around operations documented safe for the
  chosen design.
- The frame length needs a repository-approved bound before allocation.
- Tests should cancel after each partial-read boundary and verify subsequent
  drain/re-entry behavior.

## Forbidden behavior

- Infer cancellation safety merely because Rust drops values safely.
- Recommend an unbounded persistent buffer.
- Ignore shutdown ownership and post-cancellation semantics.

## Objective assertions

- The response names state ownership, resource bounds, cancellation points, and
  verification scenarios.

## Scoring

Score 0-2 for each objective dimension plus precise skill routing. Passing
requires 8/10 and no forbidden behavior.
