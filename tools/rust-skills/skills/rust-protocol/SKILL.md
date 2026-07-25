---
name: rust-protocol
description: Engineer and review binary protocols, wire formats, builders, encoders, parsers, framing, stateful messages, bit ordering, integrity, correction, malformed input, and conformance. Use for protocol or serializer code. Do not apply protocol rules to unrelated in-memory models.
---

# Engineer Rust Protocols

## Establish authority and vocabulary

1. Read `$rust-core`, the exact specification revision and errata, repository
   glossary, adopted codec conventions, existing vectors, and interoperability
   targets.
2. State bit numbering, byte order, wire order, host representation, field
   widths, reserved behavior, and the meanings of bit, symbol, dibit, octet,
   field, frame, and message.
3. Read [protocol invariants](references/protocol.md) for validation, evidence,
   partial input, correction, and resource budgets. Read
   [wire and conformance](references/wire-and-conformance.md) for representation,
   framing, state machines, encoding, and test obligations.

## Implement in layers

Keep transport buffering, frame detection, raw wire types, structural parsing,
integrity, correction, semantic types, and application trust distinguishable.
Start with a readable specification-shaped reference implementation. Add
zero-copy, buffer reuse, SIMD, or unsafe optimization only through
`$rust-performance` and `$rust-unsafe-ffi` with preserved reference evidence.

Route API design to `$rust-api-design`, testing/property/fuzz work to
`$rust-testing`, signal-processing stages to `$rust-dsp`, and concurrency around
stream delivery to `$rust-async-concurrency`.

## Verify

Use specification-derived known-answer vectors, individual-bit and cross-byte
tests, malformed and truncated cases, reserved and unknown values, resource
boundaries, state transitions, independent encoder and decoder evidence,
properties, fuzzing, and cross-implementation tests where available.
Encode/decode round trips alone are insufficient because paired inverse defects
can cancel.

## Output

Record authority, vocabulary, representation, validation and resource policy,
malformed-input behavior, evidence, deviations, and measured optimizations.
