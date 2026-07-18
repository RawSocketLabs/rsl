---
name: rsl-rust-review
description: Review Rust diffs, branches, commits, and designs for actionable correctness, regression, safety, API, concurrency, performance, testing, and maintainability risks under repository-local rules. Use for a requested code review or pre-merge risk assessment. Do not use merely to summarize code or implement an unrequested fix.
---

# RSL Rust Review

## Establish the review contract

1. Read the current request, closest repository instructions, adoption profile,
   and relevant manifests before judging the change.
2. Identify the intended behavior, affected callers, trust and unsafe boundaries,
   hot paths, generated ownership, and verification claimed by the author.
3. Inspect the full diff and enough surrounding code, tests, and history to prove
   or disprove a suspected regression. Do not infer a defect from style alone.
4. Run safe, relevant diagnostics when they materially change confidence. Never
   claim a command or platform result that was not observed.
5. Apply [the review procedure](references/review-procedure.md), then load only
   the implicated categories from [the risk checks](references/risk-checks.md).
6. Recheck each proposed finding against repository precedence, reachable input,
   actual types, callers, and existing tests. Remove speculative findings.

## Report findings first

Order actionable findings by consequence. For each finding:

- identify the smallest exact file and line range;
- state the reachable condition and resulting consequence;
- connect the consequence to the repository contract or demonstrated behavior;
- distinguish confirmed evidence from a bounded inference; and
- suggest the direction of correction without requiring an unrelated redesign.

Treat correctness, data loss, deadlock, unsoundness, resource exhaustion,
protocol corruption, and demonstrated performance regressions as findings.
Present optional simplification, naming, or adjacent cleanup separately and only
when it materially helps the owner.

After findings, list material assumptions and verification gaps. If there are no
findings, say so directly and still identify important evidence that was not
available. Do not invent an issue to populate the review.

Do not modify code, dependencies, external systems, or pull requests unless the
user separately asks for implementation.
