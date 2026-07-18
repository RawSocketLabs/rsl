# Nonmechanical Rust Style

Repository-local style wins. rustfmt and configured Clippy own mechanical rules;
use these preferences to make domain logic easier to read, not to manufacture
blocking findings.

### CORE-STYLE-001 Prefer structured, meaningful branching

- **Strength:** PREFER
- **Applies to:** control flow
- **Directive:** Use `match` for enums, `Option`, `Result`, multiple meaningful
  cases, and exhaustive state reasoning. Use `if` for direct predicates and
  `if let` when one pattern is truly the sole interesting case. Keep owned enum
  variants explicit when evolution should force a decision.
- **Exceptions:** Open external domains and preserved unknown values require a
  deliberate fallback.
- **Mechanical owner:** Human/agent review.
- **Sources:** Preference R151, R159.

### CORE-STYLE-002 Keep the successful path visible

- **Strength:** SHOULD
- **Applies to:** validation and business logic
- **Directive:** Use early returns and `let ... else` for preconditions. Use short
  combinator chains for obvious transformations; switch to `match`, named values,
  and loops when they reveal errors, state, ownership, or policy.
- **Exceptions:** Prefer one cohesive `match` over many scattered exits.
- **Mechanical owner:** Review.
- **Sources:** Preference R152, R153, R161.

### CORE-STYLE-003 Name and organize domain concepts

- **Strength:** SHOULD
- **Applies to:** functions, identifiers, and modules
- **Directive:** Extract functions around coherent concepts rather than line
  counts. Use stable vocabulary, positive booleans, domain unit types or unit
  suffixes, and capability-oriented modules instead of generic dumping grounds.
- **Exceptions:** A narrowly owned support module may use a generic name when its
  contents remain cohesive.
- **Mechanical owner:** Review.
- **Sources:** Preference R154, R156, R157.

### CORE-STYLE-004 Make ownership operations locally obvious

- **Strength:** SHOULD
- **Applies to:** mutation, shadowing, and cloning
- **Directive:** Narrow mutable scopes. Shadow only for a clear transformation.
  Use `Arc::clone`/`Rc::clone` for shared ownership and reconsider a clone added
  only to satisfy the borrow checker.
- **Exceptions:** A measured specialization may choose a different ownership path
  with a documented contract.
- **Mechanical owner:** Review and benchmarks.
- **Sources:** Preference R155, R162.

### CORE-STYLE-005 Require significant value from macros

- **Strength:** SHOULD
- **Applies to:** new and expanded macros
- **Directive:** Prefer functions, traits, and generics. Use a macro only for
  significant syntax generation, repetition reduction, compile-time structure,
  or equivalent value. Document nontrivial grammar, hygiene, and diagnostics.
- **Exceptions:** Existing repository macro conventions may justify consistent
  extension.
- **Mechanical owner:** Review and compile tests.
- **Sources:** Preference R164.

### CORE-STYLE-006 Keep visibility and exceptions narrow

- **Strength:** SHOULD
- **Applies to:** imports, visibility, lint attributes, and unsafe blocks
- **Directive:** Prefer explicit imports and private items. Use the narrowest
  visibility and lint exception with a reason; prefer checked expectations where
  supported. Keep unsafe blocks minimal with an adjacent `SAFETY` proof.
- **Exceptions:** Deliberate preludes, generated code, and conditional compilation
  may require broader scoped policy.
- **Mechanical owner:** Clippy, rustc lints, review.
- **Sources:** Preference R158, R165, R166.
