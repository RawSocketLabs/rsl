# Review Procedure

## 1. Establish scope and precedence

- Read the user request, nearest `AGENTS.md`, parent instructions, adoption file,
  manifests, and generated-file notices.
- Determine the repository profile and any component override.
- Separate intended behavior from incidental diff shape. Ask whether a breaking
  change is explicit before treating instability as a defect in a pre-1.0 repo.

## 2. Trace changed contracts

- Follow public callers, ownership transfer, error handling, lifecycle, feature
  gates, platform branches, and canonical generation paths affected by the diff.
- For input-dependent behavior, construct a reachable example rather than
  assuming an edge case.
- For concurrency, trace producer/consumer rate, capacity, overload, shutdown,
  and lock/await boundaries.
- For unsafe, state the proof obligation and verify that safe callers cannot
  violate it.

## 3. Evaluate evidence

- Read existing tests before claiming a missing guarantee.
- Distinguish compilation from native runtime evidence and correctness tests from
  performance measurements.
- Run focused commands when safe and useful. Record the exact command and result.
- Treat unsupported performance claims, blind flaky-test retries, and silently
  regenerated expectations as review risks.

## 4. Admit only actionable findings

A finding needs a reachable condition, concrete consequence, and precise code
location. Rank by consequence, then confidence. Use these broad levels:

- **Critical:** unsoundness, exploitable trust-boundary failure, or unrecoverable
  data corruption likely in supported use.
- **High:** common-path correctness failure, deadlock, protocol corruption, or
  severe resource exhaustion.
- **Medium:** bounded behavioral regression, misleading public contract, feature
  breakage, or evidence gap that can ship a real defect.
- **Low:** concrete maintainability problem likely to cause a future defect.

Do not report preference-only style, possible cleanup, or a hypothetical without
a reachable consequence as a defect. Put useful optional work after findings.

## 5. Recheck and report

- Re-read each cited line with its caller and repository rule.
- Remove duplicates and findings contradicted by types, tests, or local policy.
- Lead with findings. Then state assumptions, verification performed, and gaps.
- If no findings remain, say so plainly; do not inflate optional suggestions.
