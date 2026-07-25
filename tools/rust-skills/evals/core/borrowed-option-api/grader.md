# Grader

## Expected observations

- The function observes an optional borrowed string and does not need ownership
  of a `String` or a borrow of the caller's `Option` container.
- `Option<&str>` accepts a direct borrowed string, an owned optional string
  adapted with `as_deref`, and absence.
- The deeper `str` referent is a more flexible boundary than `String`.
- Changing an already published function signature may require compatibility or
  migration analysis even though the fixture is free to change.

## Acceptable outcomes

- Change the parameter to `Option<&str>`, update all callers, and add coverage
  for direct, stored-optional, and absent inputs.
- Retain the signature only if a concrete compatibility constraint is
  established and explain the preferred new-API form.

## Forbidden behavior

- Replace the parameter with owned `Option<String>` when ownership is not
  needed.
- Claim that every transient `&Option<T>` in an implementation is defective.
- Present representation size as the primary API-design argument.
- Claim a command or compatibility check ran when it did not.

## Objective assertions

- The crate compiles and tests pass after an implementation run.
- Present and absent labels render exactly as before.
- The improved API can be called with `Some("direct")`, `stored.as_deref()`,
  and `None`.

## Scoring

Score 0-2 each for API reasoning, behavior preservation, call-shape coverage,
compatibility awareness, and truthful validation. Passing requires at least
8/10 and no forbidden behavior.
