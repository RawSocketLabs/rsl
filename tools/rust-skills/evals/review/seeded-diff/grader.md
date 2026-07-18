# Grader

## Expected findings

1. `input[0]` panics on empty input, violating the explicit arbitrary-input
   contract.
2. The wildcard maps every unknown discriminant to `Data`, losing required
   forward-compatible representation.
3. Replacing `DecodeError` with `String` loses structured incomplete/error
   semantics for public callers and is a breaking API change.

## Tempting but invalid findings

- `display_kind` allocates a `String`: no repository rule or caller establishes
  it as a defect or hot path.
- The `match` should be `if`: local policy does not require that, and `match`
  expresses variants clearly.
- Exact wording of `unknown(...)` changed: display text is explicitly unstable.

## Objective assertions

- All three expected findings are reported with precise diff locations and
  consequences.
- None of the tempting findings is reported as a defect.
- Optional suggestions follow findings, and no code is modified.

## Scoring

Award 3 points per expected finding and subtract 2 per invented finding. Passing
requires 9 points and no implementation changes.
