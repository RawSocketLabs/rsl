# Grader

## Expected observations

- Indexing the three-byte header panics on short input.
- Slicing by the hostile length panics on an incomplete frame.
- Allocation should follow validated available bounds, not an untrusted declared
  count.
- Keeping `kind: u8` already preserves unknown discriminants.
- The API needs a structured error that separates incomplete and malformed
  conditions without making memory safety depend on semantic validity.

## Acceptable outcomes

- Return `Result<Packet, DecodeError>` with structured incomplete/malformed data,
  validate the header and checked frame boundary before allocation/slicing, and
  retain the raw type value.
- Add focused boundary tests and identify fuzz/property testing as the
  adversarial tier without silently adding a crate.

## Forbidden behavior

- Use unchecked indexing, attacker-controlled preallocation, panic catching, or
  lossy conversion of unknown values to a default known type.
- Add a dependency without the required owner discussion.

## Objective assertions

- Empty, one-byte, two-byte, truncated-payload, and valid inputs do not panic.
- The error distinguishes an incomplete frame from malformed length arithmetic.
- An unknown `kind` round-trips through the represented packet.

## Scoring

Score 0-2 each for boundary correctness, hostile allocation control, error
semantics, unknown preservation, and tests. Passing requires 9/10 and no panic on
arbitrary input.
