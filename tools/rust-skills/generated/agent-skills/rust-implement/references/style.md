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
- **Why:** An exhaustive `match` makes the compiler report every place a new
  variant needs a decision. A catch-all or a chain of `if let` silently assigns
  that variant the old default, which is how a newly added state gets handled as
  if it were the previous one.
- **Exceptions:** Open external domains and preserved unknown values require a
  deliberate fallback.
- **Mechanical owner:** Human/agent review.
- **Sources:** Preference R151, R159.

### CORE-STYLE-002 Keep the successful path visible

- **Strength:** SHOULD
- **Applies to:** validation and business logic
- **Directive:** Use early returns and `let ... else` for preconditions. Use short
  combinator chains for obvious transformations; switch to `match`, named values,
  and loops when they reveal errors, state, ownership, or policy. Extract a
  helper when its name exposes a coherent concept or contains an invariant, not
  merely to reduce indentation.
- **Why:** Preconditions handled up front leave one path carrying the actual
  work, so a reader tracks a single line of reasoning instead of holding several
  open branches. Extraction that only reduces indentation moves the same
  reasoning behind a name that explains nothing.
- **Exceptions:** Prefer one cohesive `match` over many scattered exits. A
  symmetric branch can be clearer with `if`/`else`, and a state machine can
  legitimately nest decisions. Do not impose a numeric indentation limit or
  remove every `else`.
- **Mechanical owner:** Review.
- **Sources:** Preferences R152-R154, R161, and R202; CodeAesthetic advisory
  source.

### CORE-STYLE-003 Name and organize domain concepts

- **Strength:** SHOULD
- **Applies to:** functions, identifiers, and modules
- **Directive:** Extract functions around coherent concepts rather than line
  counts. Name the domain role rather than restating an incidental storage or
  implementation type. Use stable vocabulary, positive booleans, domain unit
  types or unit suffixes, and capability-oriented modules instead of generic
  dumping grounds.
- **Why:** A name that restates the storage type has to be re-derived into domain
  meaning at every call site, and an unsuffixed quantity is how unit confusion
  enters arithmetic that the compiler will happily accept.
- **Exceptions:** A narrowly owned support module may use a generic name when its
  contents remain cohesive. Preserve conventional Rust, protocol, mathematical,
  and repository abbreviations such as `io`, `ip`, `crc`, `fft`, or an approved
  `Iq*` vocabulary; expanding a familiar term can make a name less precise.
- **Mechanical owner:** Review.
- **Sources:** Preferences R154, R156, R157, and R203; CodeAesthetic advisory
  source.

### CORE-STYLE-005 Require significant value from macros

- **Strength:** SHOULD
- **Applies to:** new and expanded macros
- **Directive:** Prefer functions, traits, and generics. Use a macro only for
  significant syntax generation, repetition reduction, compile-time structure,
  or equivalent value. Document nontrivial grammar, hygiene, and diagnostics.
- **Why:** A macro opts out of the tooling everything else relies on — type
  errors point at expansions, `go to definition` stops working, and the grammar
  is documented nowhere but the implementation. That price is worth paying for
  real syntax generation and not for a call a function could have made.
- **Exceptions:** Existing repository macro conventions may justify consistent
  extension.
- **Mechanical owner:** Review and compile tests.
- **Sources:** Preference R164.

### CORE-COMMENT-001 Preserve durable knowledge near the code

- **Strength:** SHOULD
- **Applies to:** source comments, rustdoc, `SAFETY` comments, and deferred work
- **Directive:** First use names, types, functions, modules, and clearer control
  flow to express behavior. Use comments for information code cannot enforce or
  reveal adequately: rationale, invariants, units, protocol and algorithm
  authority, performance constraints, compatibility decisions, safety proofs,
  and actionable `TODO` or `FIXME` conditions. Keep public contracts in rustdoc
  and update nearby comments when behavior changes.
- **Why:** Code records what it does, never why that was chosen over the obvious
  alternative. Once the rationale, the specification citation, or the safety
  argument is lost, the next reader either preserves the decision superstitiously
  or reverts it and rediscovers the defect it prevented.
- **Exceptions:** Remove comments that merely narrate current syntax or have
  become false. Do not remove a durable explanation merely because the code is
  described as self-documenting; `SAFETY` reasoning and required specification
  citations remain mandatory.
- **Mechanical owner:** Documentation tests where applicable and review.
- **Sources:** Preference R163; explicit owner rejection of a blanket
  no-comments rule; qualified CodeAesthetic advisory source.

### CORE-CHANGE-001 Keep task changes scoped

- **Strength:** MUST
- **Applies to:** agent-authored changes
- **Directive:** Implement the requested change and necessary supporting updates.
  Do not silently bundle unrelated cleanup, formatting churn, dependency updates,
  or speculative refactors. Surface worthwhile adjacent work as a choice.
- **Why:** Unrelated changes bundled into a diff are reviewed with the attention
  budget the requested change earned, so a real defect hides inside the cleanup.
  They also make the change hard to revert without losing the fix it carried.
- **Exceptions:** Fix an adjacent issue only when required for correctness or
  verification of the requested result, and report it.
- **Mechanical owner:** Diff review.
- **Sources:** Preference R79-R81.
