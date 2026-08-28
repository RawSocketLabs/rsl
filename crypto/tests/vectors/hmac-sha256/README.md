# HMAC-SHA-256 vector provenance

## RFC 4231

- **Evidence class:** published vector.
- **Publication:** RFC 4231, *Identifiers and Test Vectors for HMAC-SHA-224, HMAC-SHA-256,
  HMAC-SHA-384, and HMAC-SHA-512*.
- **Revision:** December 2005.
- **Status:** Proposed Standard; no errata affecting the test vectors were listed when checked.
- **Persistent identifier:** <https://doi.org/10.17487/RFC4231>.
- **Authoritative document:** <https://www.rfc-editor.org/rfc/rfc4231.html>.
- **Cases used:** §4.2 through §4.8, Test Cases 1 through 7.
- **Checked:** 2026-08-27.
- **Conversion policy:** concatenate wrapped hexadecimal lines without separators and decode each
  complete octet. ASCII annotations are explanatory only; hexadecimal `Key` and `Data` values are
  the fixtures. Test Case 5 publishes a deliberately truncated 128-bit value and must not be
  presented as a full 256-bit HMAC-SHA-256 tag.

## Construction sources

The vector suite does not replace the construction definition. Implementation behavior maps to:

- NIST FIPS 198-1, §2.3–§4 and Table 1:
  <https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf>; and
- RFC 2104, §2:
  <https://www.rfc-editor.org/rfc/rfc2104.html>.

Both sources were checked on 2026-08-27. `crypto/STANDARDS.md` records their current publication
status and the pending NIST transition from FIPS 198-1 to draft SP 800-224.

## Differential implementation

- **Evidence class:** differential result.
- **Implementation:** RustCrypto `hmac` 0.13.0 instantiated with `sha2` 0.11.0, both with default
  features disabled.
- **Upstream:** <https://github.com/RustCrypto/MACs/tree/master/hmac>.
- **Role:** development-only oracle across deterministic key and message boundary lengths.
- **Checked:** 2026-08-27.

Differential agreement is supplementary evidence. Production `rsl-crypto` code depends on
neither RustCrypto crate.
