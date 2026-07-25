# Review Procedure

## 1. Establish scope and precedence

- Read the request, closest instructions, adopted profile and decisions,
  manifests, generated ownership, intended behavior, and claimed validation.
- Identify the reviewed diff, base revision, excluded paths, public consumers,
  trust boundaries, first-class targets, and whether breaking change is
  authorized.
- Separate verified facts, author intent, repository convention, recommendation,
  and unresolved assumption.

## 2. Trace changed contracts

Follow each material change through callers, implementations, ownership,
lifetimes, mutation, errors, panics, features, target branches, persistence,
wire or ABI representations, generated paths, examples, tests, and cleanup.
Construct a reachable failure scenario before admitting a concern.

Activate the owning skills from the routing reference. Review owns consequence,
evidence, severity, confidence, and reporting. The activated skill owns its
domain vocabulary, invariants, checklist, and validation.

For every diff, ask:

- What behavior changed, including failure and cleanup?
- Who establishes and preserves each affected invariant?
- Can reachable input panic, corrupt, deadlock, leak, exhaust resources, or
  silently lose evidence?
- Did the public, feature, target, dependency, serialization, wire, ABI,
  documentation, or example contract expand?
- Does the changed evidence test the actual risk and configuration?
- Did unrelated cleanup, formatting, generation, or dependency churn enter?

## 3. Evaluate evidence

- Read existing tests before claiming a missing guarantee.
- Distinguish compilation, native runtime, correctness, coverage, performance,
  target, hardware, Miri, sanitizer, Loom, fuzz, and interoperability evidence.
- Run safe relevant diagnostics when they materially change confidence. Record
  exact commands and observed results.
- Treat an unavailable command as unavailable, never passed.
- Reject performance evidence whose timed iterations change the workload,
  protocol evidence supported only by paired round trips, flaky success obtained
  only by blind retry, and regenerated expectations accepted without review.

## 4. Admit only actionable findings

A finding requires an exact location, reachable condition, concrete consequence,
repository or behavioral contract, correction direction, and verification.
Rank by consequence and state confidence separately.

- **Critical:** reachable unsoundness, exploitable trust-boundary failure, or
  unrecoverable corruption in supported use.
- **High:** common-path correctness failure, deadlock, protocol corruption,
  severe resource exhaustion, or data loss.
- **Medium:** bounded behavioral regression, misleading durable contract,
  supported feature or target break, or evidence gap that can ship a real
  defect.
- **Low:** concrete maintainability defect with a plausible future failure.

Confidence is evidence quality, not severity:

- **High:** the changed code, reachable caller, governing contract, and failure
  consequence directly establish the finding.
- **Medium:** the failure is well-supported but one material premise depends on
  an unobserved supported environment, input, or downstream caller.
- **Low:** a credible risk still needs a named fact or reproduction. Present it
  as an evidence gap or question, not a blocking fact.

Critical, high, and medium behavioral findings normally block merge until
corrected, explicitly accepted by the owner, or disproven. A low-severity
finding blocks only when repository policy makes that contract mandatory.
Optional suggestions follow findings and must state their concrete benefit;
they are never disguised as defects.

Do not report preference-only style, rustfmt output, possible cleanup,
unmeasured performance, hypothetical abstraction, blanket clone or test
`unwrap` objections, or a concern contradicted by types, callers, tests, or
local policy. Do not enforce numeric function or indentation limits, ban every
`else`, expand established domain abbreviations, remove durable comments, turn
every loop into an iterator chain, or demand a trait solely for mocking. A
readability suggestion must identify the concrete behavior, invariant, change
boundary, or maintenance hazard it makes easier to see.

## 5. Recheck and report

Re-read every cited location with its caller and governing rule. Remove
duplicates and speculation. Lead with findings in severity order using the
schema in `SKILL.md`. Then state assumptions, exact verification, and gaps. If
no findings remain, say so plainly and do not manufacture optional comments.
