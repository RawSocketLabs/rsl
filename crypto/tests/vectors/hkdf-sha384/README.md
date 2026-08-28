# HKDF-SHA-384 vector provenance

RFC 5869 (recorded in `tests/vectors/hkdf-sha256/README.md`) publishes vectors only for SHA-256
and SHA-1. Published evidence for SHA-384 therefore comes from Project Wycheproof.

## Project Wycheproof

- **File:** `testvectors_v1/hkdf_sha384_test.json` from <https://github.com/C2SP/wycheproof>.
- **SHA-256 of file as downloaded 2026-08-28 from the `master` branch:**
  `69ff6ea3657bb9c1b8cdffbbb4e7832353d08fd15c0d9997b03f7a6b180e3678`.
- **Cases:** all 83 (80 valid; 3 invalid, each requesting more than 255 · 48 bytes).
- **Fields:** `ikm`, `salt`, `info`, `size`, `okm`, `result`; copied verbatim into
  `tests/hkdf_sha384/wycheproof_fixtures.rs`.
- **License:** Apache License 2.0.

The `hkdf` crate 0.13 with `sha2::Sha384` is the development-only differential oracle.
