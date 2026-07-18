# Documentation and Examples

### CORE-DOC-001 Document the public and conceptual contract

- **Strength:** MUST
- **Applies to:** reusable public libraries
- **Directive:** Document public items and applicable `# Errors`, `# Panics`, and
  `# Safety`. Use module docs to teach purpose, domain vocabulary, invariants,
  data flow, and a progressive path from common to expert use.
- **Exceptions:** Generated code and raw bindings may use scoped, explained lint
  exceptions.
- **Mechanical owner:** rustdoc, doctests, missing-docs lint, review.
- **Sources:** Preference R63-R65.

### CORE-DOC-002 Cite protocol authority precisely

- **Strength:** MUST
- **Applies to:** behavior derived from a protocol or standard
- **Directive:** Cite the defining document, revision, and exact section, table,
  or figure near the behavior it controls. Keep reference implementations
  subordinate to the specification.
- **Exceptions:** Record ambiguity explicitly when no stable authority exists.
- **Mechanical owner:** Review and reference-vector tests.
- **Sources:** Preference R66, R83, R84.

### CORE-EXAMPLE-001 Give each runnable example one consumer use case

- **Strength:** MUST
- **Applies to:** `examples/` targets
- **Directive:** Use a task-oriented name and source-level statement of purpose,
  prerequisites, invocation, expected behavior, and intentional omissions.
  Examples teach workflows; tests own edge cases and regressions.
- **Exceptions:** A few sanity assertions may clarify the taught invariant.
- **Mechanical owner:** Example inventory review and CI compilation.
- **Sources:** Preference R138, R148.

### CORE-EXAMPLE-002 Compile real, production-shaped examples

- **Strength:** MUST
- **Applies to:** substantial rustdoc and examples
- **Directive:** Use the supported public API, `Result` and `?` for normal failure,
  real feature gates, and transparent compiled source. Prefer execution in CI;
  reserve `no_run` and `ignore` for documented environmental constraints.
- **Exceptions:** Omit orthogonal setup explicitly; never replace essential
  behavior with a `TODO`.
- **Mechanical owner:** Doctests, `cargo check --examples`, feature CI.
- **Sources:** Preference R140, R141, R145-R147.

### CORE-EXAMPLE-003 Teach flexibility without hiding operational cost

- **Strength:** SHOULD
- **Applies to:** protocol, DSP, and performance-sensitive examples
- **Directive:** Lead with valid/default and shortest-correct use. Label protocol
  validation escape hatches. Use deterministic hardware-independent DSP data
  first. Expose material allocation, copy, blocking, runtime, and hardware costs.
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
- **Exceptions:** Keep local reasoning next to code when it does not justify a
  separate document.
- **Mechanical owner:** Review.
- **Sources:** Preference R67, R163.
