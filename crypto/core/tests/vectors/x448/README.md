# X448 vector provenance

- **Controlling publication:** RFC 7748, *Elliptic Curves for Security*, January 2016, IRTF
  Informational, §5.2 and §6.2. <https://www.rfc-editor.org/info/rfc7748/>. The X25519 record
  (`tests/vectors/x25519/README.md`) lists the RFC 7748 errata; none change the X448 vectors.

## Material used

- §5.2: both direct X448 scalar/u-coordinate/output triples, and the iterated results after one,
  1,000 and 1,000,000 applications (the last is an `#[ignore]`d test, run with `--ignored`).
- §6.2: Alice's and Bob's private keys, public keys, and common shared secret.

## Project Wycheproof

- **File:** `testvectors_v1/x448_test.json` from <https://github.com/C2SP/wycheproof>.
- **SHA-256 and access date:** not recorded when the cases were imported. The JSON file is not
  committed; its cases are transcribed into `tests/x448/wycheproof_fixtures.rs`.
- **Cases:** all 510 (253 valid, 245 acceptable, 12 invalid). Eleven all-zero shared secrets are
  rejected by the API, and the twelve over-long public keys are unrepresentable.
- **License:** Apache License 2.0.

## Conversion policy

Printed hexadecimal is joined across line breaks in printed byte order, without reversal;
little-endian interpretation happens only inside X448. No differential crate is used: the
RustCrypto `x448` crate is pre-release only, so Wycheproof's independently generated suite is the
scheme-level oracle (`STANDARDS.md`).
