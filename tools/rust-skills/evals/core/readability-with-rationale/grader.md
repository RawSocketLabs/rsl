# Grader

## Expected observations

- Three independent rejection conditions surround the successful queue
  mutation and can be expressed as guard clauses.
- The process-memory comment records why `max_pending` is not a wire-format
  limit; names and types do not preserve that distinction.
- The existing function is cohesive. It does not need tiny extracted helpers,
  a dependency-injection framework, a trait, or a functional iterator rewrite.
- Public names and behavior do not need to change.

## Acceptable outcomes

- Flatten the independent preconditions with early returns, retain the durable
  bound rationale adjacent to its check, preserve the successful path, and run
  the crate tests.
- Retain some nesting if the agent gives a concrete readability reason and
  still preserves all behavior and rationale.

## Forbidden behavior

- Delete the durable comment merely because comments are considered undesirable.
- Claim that nesting, `else`, loops, or comments are universally prohibited.
- Split the function into trivial helpers solely to meet a numeric size or
  indentation rule.
- Add a trait, generic, dependency, or public API change without a demonstrated
  need.
- Rename established public vocabulary or mix unrelated cleanup into the diff.
- Claim a command ran when it did not.

## Objective assertions

- The crate compiles and all tests pass after an implementation run.
- Empty, missing-content-type, full-queue, and successful cases behave exactly
  as before.
- Every rejected upload leaves the queue unchanged.
- The process-memory versus wire-limit rationale remains recorded near the
  bound check.

## Scoring

Score 0-2 each for control-flow clarity, behavior preservation, rationale
preservation, scope discipline, and truthful validation. Passing requires at
least 8/10 and no forbidden behavior.
