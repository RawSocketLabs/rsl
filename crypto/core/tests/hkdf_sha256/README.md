# HKDF-SHA-256 integration tests

This directory holds public-behavior tests compiled by the `tests/hkdf_sha256.rs` harness:

- `known_answers.rs` — RFC 5869 Appendix A.1–A.3 `PRK` and `OKM` values;
- `boundaries.rs` — zero-length output, one block, partial final block, exactly 255 blocks, and
  rejection above `255 * HashLen` before any output mutation;
- `boundaries.rs` also proves explicit Extract followed by Expand agrees with the convenience
  operation; and
- `differential.rs` — comparison with an established development-only implementation.

Private recurrence and counter tests remain beside the layer that owns them. Published-vector
provenance is recorded in `tests/vectors/hkdf-sha256/README.md`.
