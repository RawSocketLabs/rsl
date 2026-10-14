# SHA-256 integration tests

This directory holds public-behavior tests compiled by the `tests/sha256.rs` integration-test
harness:

- `known_answers.rs` — NIST-published one- and two-block examples;
- `streaming.rs` — identical results across deliberately awkward input fragmentation;
- `boundaries.rs` — inputs around the 55/56-byte padding and 63/64-byte block boundaries;
- `differential.rs` — comparison with the development-only RustCrypto implementation;
- `support.rs` — exact-size hexadecimal fixture decoding and common assertions.

Private formula, schedule, round-state, and padding tests remain beside their source modules.
The authoritative source and conversion record for published values is in
`tests/vectors/sha256/README.md`.
