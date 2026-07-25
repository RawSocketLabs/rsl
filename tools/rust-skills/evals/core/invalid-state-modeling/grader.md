# Grader

## Expected observations

- `settled`, `confirmation`, and `failure` encode one progress state across
  three fields, so settled-with-a-failure, settled-without-a-confirmation, and
  failed-and-settled are all constructible. `fail` leaves `settled` unchanged
  and `settle` leaves `failure` in place, so the contradiction is reachable
  through the existing API rather than only by direct field assignment.
- One enum with `Pending`, `Settled { confirmation }`, and `Failed { failure }`
  removes those combinations and puts each payload in the state that owns it.
  The confirmation-missing branch of `statement_line` then has nothing to
  render because it can no longer occur.
- `account` and `reference` are independent attributes that hold in every
  state and remain ordinary fields.
- `amount_dollars` is a binary float used for an exact monetary quantity and
  carries no currency. Integer minor units with an explicit currency remove
  representation drift from summation; the current test values happen to be
  exactly representable, which is why the defect does not show up in the
  existing assertions.
- Public fields let any caller reconstruct an invalid combination, so the
  improvement depends on making the fields private and the transitions the only
  way to change state.

## Acceptable outcomes

- Replace the three progress fields with one enum, keep `account` and
  `reference` as fields, make the fields private or otherwise prevent
  contradictory construction, and match exhaustively when rendering.
- Represent the amount as an exact integer minor-unit type with a currency, or
  state explicitly why the repository's existing money representation is being
  kept and leave the amount unchanged.
- Keep runtime errors for conditions the type cannot prove, such as a
  settlement attempt that the payment processor rejects.
- Note that this is an internal library whose callers are in the repository, so
  the signature change needs caller updates rather than a compatibility policy.

## Forbidden behavior

- Convert `account` or `reference` into enum variants or newtypes without a
  stated invariant, or otherwise rewrite every field as a wrapper type.
- Introduce type-state, marker types, or a generic state parameter for this
  record, where the states are data the caller stores and compares rather than
  a staged capability.
- Keep the enum while also keeping the `settled` boolean or a public field that
  re-admits the removed combinations.
- Change rendered statement lines or the settled total.
- Claim a command, test, or compatibility check ran when it did not.

## Objective assertions

- The crate compiles and tests pass after an implementation run.
- Pending, settled, and failed records render exactly as before.
- The settled total equals the previous value for the same inputs.
- No API path produces a record that is both settled and failed.

## Scoring

Score 0-2 each for state modeling, restraint on fields that were already
correct, quantity representation, construction and mutation control, and
truthful validation. Passing requires at least 8/10 and no forbidden behavior.
