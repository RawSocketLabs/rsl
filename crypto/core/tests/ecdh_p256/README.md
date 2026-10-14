# ECDH P-256 public test organization

These tests exercise only `EcdhP256`, `EcdhP256PrivateKey`, `EcdhP256PublicKey`, and
`EcdhP256SharedSecret`. Field, scalar, and point evidence remains beside the implementation in
`src/curve/p256/` so each layer can be compared directly with its publication.

- `known_answers.rs` contains **published evidence** from RFC 5903 §8.1 and all 25 NIST CAVP
  ECC CDH primitive P-256 cases (`cavp_cdh_fixtures.rs`).
- `boundaries.rs` contains **published evidence** from the 12 NIST CAVP PKV P-256 cases
  (`cavp_pkv_fixtures.rs`) and **standard-derived evidence** for prefix, range, curve-equation,
  private-scalar range, candidate-testing generation, negation, exact wire length, and the
  generic `KeyAgreement` path.
- `differential.rs` contains **differential evidence** against the development-only `p256`
  crate 0.14.0 over public derivation, agreement, and point parsing.

Exact publication, archive checksum, and conversion details live in
[`../vectors/ecdh-p256/README.md`](../vectors/ecdh-p256/README.md). Passing these tests is not an
audit, CAVP validation, or side-channel-resistance claim.
