# ECDSA P-256 public test organization

These tests exercise only `EcdsaP256VerifyingKey` and `EcdsaP256Signature`. Field, scalar, and
point evidence remains beside the implementation in `src/curve/p256/`, and the FIPS 186-5 §6.4.2
step sequence has focused range tests beside `src/signature/ecdsa_p256/`.

- `known_answers.rs` contains **published evidence** from RFC 6979 A.2.5 (both SHA-256
  signatures) and all 15 NIST CAVP SigVer `[P-256,SHA-256]` cases (`cavp_sigver_fixtures.rs`),
  reproducing each printed pass/fail verdict.
- `boundaries.rs` contains **standard-derived evidence** for `r`/`s` range rejection, the
  complementary `n - s` value, bit-level tampering, wrong-key rejection, malformed keys, and
  exact wire lengths.
- `differential.rs` contains **differential evidence**: 32 signatures produced by the
  development-only `p256` crate 0.14.0 verify here, and tampered copies do not.

Exact publication, archive checksum, and conversion details live in
[`../vectors/ecdsa-p256/README.md`](../vectors/ecdsa-p256/README.md). Passing these tests is not an
audit, CAVP validation, or side-channel-resistance claim.
