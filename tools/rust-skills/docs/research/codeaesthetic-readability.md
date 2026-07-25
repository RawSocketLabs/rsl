# CodeAesthetic Readability Synthesis

Status: reviewed synthesis implemented in the modular runtime packages

## Provenance and authority

The eight videos listed on the CodeAesthetic channel on 2026-07-25 were
reviewed from their YouTube metadata and captions. They are advisory essays,
not Rust authorities. No wording is copied into runtime guidance. Repository
decisions, organization vocabulary, Rust semantics, and measured evidence have
precedence.

The review deliberately separates a useful design pressure from the video's
title or strongest formulation. Memorable slogans can start an investigation;
they are not mechanical review rules.

## Catalog disposition

| Video | Portable lesson | Qualification |
|---|---|---|
| `Abstraction Can Make Your Code Worse` | Charge an abstraction for the coupling it creates; require a named boundary or separated decision. | Repetition is evidence to inspect, not proof that either duplication or abstraction is correct. A second use case is useful evidence but not a universal minimum. |
| `Naming Things in Code` | Name domain roles and units; avoid vague `utils`, `common`, and `helper` buckets; let difficult naming expose weak decomposition. | Preserve standard, protocol, mathematical, and repository vocabulary even when abbreviated. Rust type names, ownership forms, and unit suffixes may communicate facts that are not otherwise visible at a call site. |
| `Why You Shouldn't Nest Your Code` | Use guard clauses, `let ... else`, and concept-level extraction when they keep the successful path visible. | Do not impose a numeric indentation limit or remove every `else`. A cohesive `match`, symmetric branch, or state transition can be clearer than scattered exits or tiny helpers. |
| `The Flaws of Inheritance` | Prefer composition of focused capabilities and keep shared contracts narrow. | Rust has no class inheritance. Traits, generics, enums, closures, and ordinary concrete composition each have different costs; do not translate an inheritance critique into trait proliferation. |
| `Don't Write Comments` | First try names, types, functions, and simpler control flow instead of comments that narrate syntax. Keep public contracts near code. | The blanket thesis is rejected. Preserve rationale, invariants, units, protocol and algorithm sources, performance constraints, `SAFETY` proofs, compatibility notes, and actionable deferred work when code cannot express them. |
| `Premature Optimization` | Define the real workload, profile, measure before and after, and prioritize material algorithm or data-layout changes. | Performance is a top-tier concern in declared hot paths. Existing requirements, asymptotic defects, and known resource bounds can justify investigation before production pain appears, but not unsupported speed claims. |
| `Dependency Injection, The Best Pattern` | Make required dependencies visible and separate construction policy from use when that boundary improves substitution, configuration, or testing. | Passing a value is already injection; it need not introduce a framework or trait. Do not create one trait per dependency solely to mock it. Select concrete types, generics, enums, closures, or trait objects from real variability and ownership needs. |
| `Dear Functional Bros` | Prefer pure helpers and iterator pipelines for clear transformations with controlled state and effects. | Keep explicit loops and named state for fallible, interruptible, stateful, side-effecting, or ownership-sensitive work. Fewer lines or fewer loops is not by itself an improvement. |

## Implemented ownership

- `rust-core` owns abstraction cost, composition, and explicit dependency
  boundaries.
- `rust-implement` owns naming, control-flow shape, comments, and selection
  between iterator pipelines and explicit loops.
- `rust-performance` owns measurement and performance evidence.
- `rust-review` prevents these preferences from becoming low-value or
  speculative findings and routes durable API consequences to
  `rust-api-design`.
- `rust-testing` owns the evidence needed to preserve behavior through a
  readability refactor.

## Review questions

- What domain decision does the new name, function, module, or abstraction make
  easier to see?
- What changes independently after the proposed boundary is introduced?
- Does the dependency boundary express real variability, ownership, or
  configuration, or only make a mock framework convenient?
- Does flattening reveal the successful path, or fragment one cohesive
  decision?
- Which comment content cannot be enforced or expressed by names, types, and
  tests?
- Is a functional transformation easier to audit for errors, state, effects,
  and early termination than the loop it replaces?
- What workload and measurement support a performance-motivated readability
  cost?
