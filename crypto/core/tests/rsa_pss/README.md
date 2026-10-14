# RSASSA-PSS public test organization

These tests exercise only `RsaPssSha256VerifyingKey` and `RsaPssSignature`. MGF1 and
EMSA-PSS-VERIFY step evidence remains beside the implementation in `src/signature/rsa_pss/`, and
the shared RSA integer engine's differential evidence remains beside `src/rsa/integer.rs`.

- `known_answers.rs` contains **published evidence**: all 18 NIST CAVP `SigVerPSS`
  2048/SHA-256 verdicts and all 10 `SigGenPSS` 2048/SHA-256 signatures (`cavp_fixtures.rs`,
  exercising the explicit 20-byte salt length), and all 108 Project Wycheproof
  `rsa_pss_2048_sha256_mgf1_32` results (`wycheproof_fixtures.rs`), which include modified
  paddings, changed salt lengths, special-case hashes, and a PKCS #1 v1.5 wrong-primitive case.
- Differential evidence for the modular exponentiation is white-box against `num-bigint-dig`;
  the Wycheproof suite serves as the independent scheme-level oracle.

Exact publication, archive checksum, and conversion details live in
[`../vectors/rsa-pss/README.md`](../vectors/rsa-pss/README.md). Passing these tests is not an
audit or CAVP validation claim.
