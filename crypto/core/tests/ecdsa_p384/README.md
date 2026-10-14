# ECDSA P-384 public test organization

These tests exercise only `EcdsaP384SigningKey`, `EcdsaP384VerifyingKey`, and
`EcdsaP384Signature`. Field, scalar, and point evidence remains beside the implementation in
`src/curve/p256/`; the RFC 6979 published `k` values and the CAVP SigGen `(d, k) -> (r, s)` cases
are white-box tests beside `src/signature/ecdsa_p384/` because `k` cannot be injected through the
public API.

- `known_answers.rs` contains **published evidence**: RFC 6979 A.2.6's SHA-384 signatures are
  reproduced exactly by deterministic signing and accepted by verification; all 15 NIST CAVP
  SigVer `[P-384,SHA-384]` verdicts (`cavp_sigver_fixtures.rs`) are reproduced; all 15 CAVP
  SigGen cases (`cavp_siggen_fixtures.rs`) derive their published points and verify.
- `boundaries.rs` contains **standard-derived evidence** for `r`/`s` range rejection, the
  complementary `n - s` value, bit-level tampering, wrong-key rejection, malformed keys, exact
  wire lengths, signing-key range, candidate-testing generation, and the generic `Signer` path.
- `differential.rs` contains **differential evidence** against the development-only `p256`
  crate 0.14.0: its signatures verify here, tampered copies do not, and deterministic signatures
  are byte-identical in both directions over 32 cases.

Exact publication, archive checksum, and conversion details live in
[`../vectors/ecdsa-p384/README.md`](../vectors/ecdsa-p384/README.md). Passing these tests is not an
audit, CAVP validation, or side-channel-resistance claim.
