# AES-128 vector provenance

## Controlling-standard example

- **Evidence class:** published vector and published intermediate states.
- **Publication:** NIST FIPS 197-upd1, *Advanced Encryption Standard (AES)*.
- **Revision:** Published November 26, 2001; updated May 9, 2023.
- **Persistent identifier:** <https://doi.org/10.6028/NIST.FIPS.197-upd1>.
- **Authoritative document:**
  <https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf>.
- **Material used:** Appendix A.1's AES-128 key expansion and Appendix B's AES-128 input,
  key, round states, round keys, and output.
- **Checked:** 2026-08-27.
- **Conversion policy:** spaces and line wrapping in hexadecimal byte sequences are removed; each
  displayed pair of hexadecimal digits becomes one byte without reversing either byte order or
  state rows. Values printed as a state matrix are retained as rows. Values printed as input,
  output, keys, or words are retained in their published sequence order.

Appendix A.1 supplies all 44 expected key-schedule words. Appendix B supplies the state-mapping,
initial key-addition, first-round transformation boundaries, round keys, and complete encryption
known answer. Each consuming white-box test cites the exact appendix boundary at the test site.

## NIST supplementary AES-128 example

- **Evidence class:** published vector and published intermediate states.
- **Publisher:** National Institute of Standards and Technology.
- **Index:**
  <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>.
- **Document:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_Core128.pdf>.
- **Material used:** AES-128 ECB encryption and decryption keys, blocks, key additions, and
  per-round substitution, row-shift, and column-mix values.
- **Checked:** 2026-08-27.

This supplementary document is validation evidence, not the controlling definition of AES.
Production code is derived from FIPS 197-upd1. Any test that consumes this material will name the
direction, block, round, and printed intermediate boundary at the test site.

All four published AES-128 ECB plaintext/ciphertext pairs are exercised through the public API in
both encryption and decryption directions. Selected printed inverse-round boundaries also test the
private inverse transformations independently.

## Differential oracle

- **Evidence class:** differential comparison, not published-vector evidence.
- **Implementation:** RustCrypto `aes` 0.9.2, development dependency only.
- **Project:** <https://github.com/RustCrypto/block-ciphers>.
- **API documentation:** <https://docs.rs/aes/0.9.2/aes/>.
- **Checked:** 2026-08-27.

The differential test compares both directions for 192 deterministic key/block pairs. RustCrypto
is neither a production dependency nor a standards authority; it is an independent implementation
used to detect agreement failures beyond the small collection of published examples.
