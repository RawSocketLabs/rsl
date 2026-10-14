# HMAC-SHA-384 vector provenance

## RFC 4231

- **Publication:** RFC 4231, *Identifiers and Test Vectors for HMAC-SHA-224, HMAC-SHA-256,
  HMAC-SHA-384, and HMAC-SHA-512*, December 2005, Proposed Standard.
  <https://doi.org/10.17487/RFC4231>; <https://www.rfc-editor.org/rfc/rfc4231.txt>.
- **Cases used:** §4.2–§4.8, Test Cases 1–7, `HMAC-SHA-384` values. Test Case 5 publishes only
  the leftmost 128 bits and is checked as a prefix of the full 48-byte tag.
- **Checked:** 2026-08-28.
- **Conversion:** a mechanical script joins wrapped hexadecimal lines and discards ASCII
  annotations; Test Case 3's 20-byte `0xaa` key is printed in prose and transcribed as such.

Construction sources (FIPS 198-1, RFC 2104) are recorded in `tests/vectors/hmac-sha256/README.md`.
The white-box long-key test takes SHA-384 of a 129-byte key from the development-only `sha2`
oracle and is labeled differential evidence. The `hmac` crate 0.13 is the differential oracle.
