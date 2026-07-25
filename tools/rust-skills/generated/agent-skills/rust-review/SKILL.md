---
name: rust-review
description: Review Rust diffs, commits, branches, and designs for actionable correctness, soundness, security, compatibility, concurrency, protocol, performance, testing, and maintainability risks. Use for review or pre-merge assessment. Do not modify code unless separately requested.
---

# Review Rust Changes

## Establish scope

1. Read `$rust-core`, the request, closest repository instructions, profile,
   manifests, intended behavior, and claimed verification.
2. Inspect the full diff and enough callers, implementations, tests, generated
   paths, and history to prove or reject a suspected issue.
3. Use [the review procedure](references/review-procedure.md) to trace changed
   contracts and [risk routing](references/risk-checks.md) only for categories
   implicated by the diff.
4. Activate the owning domain or technique skill for detailed checks. Review
   owns finding quality and prioritization, not duplicated domain rules.

## Admit findings

Report only reachable, evidence-based issues with a concrete consequence.
Prioritize correctness, soundness, security, corruption or data loss,
concurrency, error behavior, compatibility, protocol conformance, performance,
maintainability, idiomaticity, then style.

Use this schema:

```text
Severity:
Confidence:
Category:
Location:
Observed behavior:
Failure scenario:
Why it matters:
Recommended correction:
Verification:
Related skill:
```

Reject vague “consider” comments, unmeasured performance claims, rustfmt
comments, iterator rewrites that reduce clarity, abstractions without a real
second use case, blanket clone or test-`unwrap` objections, speculative claims
presented as facts, and comments unrelated to the requested change.

## Verify

Before reporting, re-check each admitted finding against the code. Confirm the
path is reachable, the claimed behavior follows from what the diff actually
does, and the recommended correction compiles against the real signatures. Drop
a finding you cannot demonstrate, or state it as a question rather than a
defect. Run the repository's declared checks when a finding depends on them.

## Output

Lead with findings ordered by consequence. Then state assumptions, exact
verification observed, and evidence gaps. If no findings remain, say so plainly.
