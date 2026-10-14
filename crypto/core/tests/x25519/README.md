# X25519 public test organization

These tests exercise only `X25519`, `X25519PrivateKey`, `X25519PublicKey`, and
`X25519SharedSecret`. Private field, scalar-preparation, and ladder evidence remains beside the
implementation so each layer can be compared directly with RFC 7748.

- `known_answers.rs` contains **published evidence** from RFC 7748 §5.2 and §6.1.
- `boundaries.rs` contains **standard-derived evidence** for high-bit masking, non-canonical
  reduction, all-zero rejection, exact wire length, and the generic `KeyAgreement` path.
- `differential.rs` contains **differential evidence** against the development-only
  `x25519-dalek` 3.0.0 implementation over public derivation, agreement, and arbitrary coordinate
  encodings.

Exact publication, errata, conversion, and fixture details live in
[`../vectors/x25519/README.md`](../vectors/x25519/README.md). Passing these tests is not an audit,
formal validation, or side-channel-resistance claim.
