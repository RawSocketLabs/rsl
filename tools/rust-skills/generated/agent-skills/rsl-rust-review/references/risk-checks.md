# Rust Review Risk Checks

Use only categories implicated by the diff.

## Public API and ownership

- Are important invalid states excluded without making normal use cumbersome?
- Did the change accidentally expand exports, features, trait commitments, or
  panic behavior?
- Could borrowing or ownership transfer replace unnecessary sharing or cloning?
- Are buffer reuse, allocation, and copy claims accurate on declared hot paths?

## Errors and hostile input

- Are lengths validated before indexing, arithmetic, or allocation?
- Can reachable operational input trigger panic, resource amplification, or
  memory-safety failure?
- Do structured errors preserve incomplete/malformed, retry, overload, and source
  information callers need?
- Does any validity escape hatch bypass only its declared checks while remaining
  memory safe?

## Concurrency and lifecycle

- Are queues bounded, and is drop/block/coalesce/backpressure policy explicit?
- Is a lock held across `.await`, callback, blocking work, or an uncontrolled
  operation?
- Who owns tasks/threads, cancellation, buffer return, and graceful shutdown?
- Does optional Tokio/Rayon integration leave runtime and pool ownership with the
  application?

## Protocol and DSP

- Does behavior cite the correct specification revision and preserve unknown or
  invalid representations where promised?
- Are bit/byte order, verbatim versus canonical encoding, integrity, and semantic
  validation kept distinct?
- Does a DSP change preserve scalar-reference, chunking, discontinuity, rate,
  numeric, alignment, and allocation contracts?
- Is architecture specialization guarded by a correct fallback and native tests?

## Unsafe, FFI, and performance

- Is unsafe explicitly permitted, necessary, tightly scoped, and justified by a
  real invariant rather than a restatement of operations?
- Can safe callers violate aliasing, lifetime, initialization, unwind, or ABI
  assumptions?
- Does performance complexity have a defined workload, before/after evidence,
  local reproduction command, and unchanged correctness contract?
- Are aggressive SIMD, layout, parallel, or unsafe techniques contained so the
  fallback remains understandable?

## Tests, docs, examples, and change scope

- Does evidence test the changed property at the right layer and feature/target?
- Is a fixed defect preserved as a minimized regression? Do interchangeable
  implementations share conformance tests?
- Do public docs, errors/panics/safety sections, examples, fixtures, and generated
  files change with the contract?
- Is every `examples/` target a distinct consumer use case rather than a hidden
  test or benchmark?
- Did unrelated cleanup, dependency churn, or formatting enter the diff without
  necessity or owner choice?
