# HMAC-SHA-256 integration tests

This directory holds public-behavior tests compiled by the `tests/hmac_sha256.rs` harness:

- `known_answers.rs` — the seven published RFC 4231 cases, with full-output cases kept distinct
  from the intentionally truncated case;
- `streaming.rs` — identical tags across awkward input fragmentation and common byte-like inputs;
- `verification.rs` — correct tags succeed and wrong value/length tags fail without exposing a
  computed tag; and
- `differential.rs` — comparison with an established development-only implementation.

Private key-normalization, pad-derivation, and inner/outer-composition tests remain beside their
owning source modules. Published-vector provenance is recorded in
`tests/vectors/hmac-sha256/README.md`.
