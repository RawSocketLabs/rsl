# Documentation and Examples

### CORE-DOC-001 Document the public and conceptual contract

- **Strength:** MUST
- **Applies to:** reusable public libraries
- **Directive:** Document public items and applicable `# Errors`, `# Panics`, and
  `# Safety`. Use module docs to teach purpose, domain vocabulary, invariants,
  data flow, and a progressive path from common to expert use.
- **Why:** Item-by-item documentation tells a reader what each name does but
  never which one to reach for first, so the concepts and vocabulary are learned
  by guessing. Undocumented panic and error conditions are worse: callers
  discover them in production because nothing said they existed.
- **Exceptions:** Generated code and raw bindings may use scoped, explained lint
  exceptions.
- **Mechanical owner:** rustdoc, doctests, missing-docs lint, review.
- **Sources:** Preference R63-R65.

### CORE-EXAMPLE-001 Give each runnable example one consumer use case

- **Strength:** MUST
- **Applies to:** `examples/` targets
- **Directive:** Use a task-oriented name and source-level statement of purpose,
  prerequisites, invocation, expected behavior, and intentional omissions.
  Examples teach workflows; tests own edge cases and regressions.
- **Why:** An example is copied into real code far more often than it is read as
  documentation. One that covers several use cases at once, or omits setup
  silently, is copied whole — including the parts that did not apply.
- **Exceptions:** A few sanity assertions may clarify the taught invariant.
- **Mechanical owner:** Example inventory review and CI compilation.
- **Sources:** Preference R138, R148.

### CORE-EXAMPLE-003 Teach flexibility without hiding operational cost

- **Strength:** SHOULD
- **Applies to:** protocol, DSP, and performance-sensitive examples
- **Directive:** Lead with valid/default and shortest-correct use. Label protocol
  validation escape hatches. Use deterministic hardware-independent DSP data
  first. Expose material allocation, copy, blocking, runtime, and hardware costs.
- **Why:** Whatever an example shows first becomes the pattern in downstream
  code. Leading with a relaxed validation policy or an allocation-heavy
  convenience path ships that choice into repositories whose authors never saw
  the tradeoff being made.
- **Exceptions:** Put hardware integration in a separate, actionable example with
  cleanup and a simulation path when practical.
- **Mechanical owner:** Review and example execution.
- **Sources:** Preference R142-R144, R149, R150.

### CORE-DOC-003 Record consequential rationale

- **Strength:** SHOULD
- **Applies to:** difficult-to-reverse architecture and non-obvious invariants
- **Directive:** Put durable context, alternatives, consequences, and evidence in
  a concise design note or ADR. Comment why, units, authority, safety, or measured
  constraints rather than narrating syntax.
- **Why:** A hard-to-reverse decision is revisited by people who were not there,
  and without the alternatives and evidence they cannot tell a deliberate
  constraint from an accident. They then either preserve it without knowing why
  or undo it and rediscover the problem it solved.
- **Exceptions:** Keep local reasoning next to code when it does not justify a
  separate document.
- **Mechanical owner:** Review.
- **Sources:** Preference R67, R163.
