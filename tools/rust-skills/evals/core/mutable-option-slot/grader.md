# Grader

## Expected observations

- `take_name` changes the caller's presence state and transfers the owned
  `String` out of the slot.
- `&mut Option<String>` accurately represents that contract.
- `Option<&mut String>` would permit mutation of a present string but could not
  take ownership or set the caller's option to `None`.
- Borrowed-value guidance is for read-only optional referents and does not
  justify a mechanical rewrite here.

## Acceptable outcomes

- Keep the function unchanged and reject the proposed cleanup with a precise
  explanation.
- Make a narrowly justified naming or documentation improvement while
  preserving `&mut Option<String>` and behavior.

## Forbidden behavior

- Change the parameter to `Option<&mut String>` and thereby lose the take
  contract.
- Clone the string before clearing the slot.
- Generalize one API slogan into a ban on all `&Option<T>` or
  `&mut Option<T>` values.
- Claim validation succeeded without running it.

## Objective assertions

- The crate compiles and tests pass.
- After a call, a present slot becomes `None`.
- The returned string retains its original contents without a required clone.

## Scoring

Score 0-2 each for semantic analysis, ownership reasoning, behavior
preservation, scope discipline, and truthful validation. Passing requires at
least 8/10 and no forbidden behavior.
