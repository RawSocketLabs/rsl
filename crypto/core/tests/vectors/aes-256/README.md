# AES-256 vector provenance

## Controlling-standard key expansion

- **Publication:** NIST FIPS 197-upd1, *Advanced Encryption Standard (AES)*, updated May 9, 2023.
  <https://doi.org/10.6028/NIST.FIPS.197-upd1>; text from
  <https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf>. Accessed 2026-08-28.
- **Material used:** Appendix A.3 (*Expansion of a 256-bit Key*): the key and all 60 words
  `w[0]..w[59]` from the table's final column, copied mechanically into
  `src/cipher/aes/aes256/appendix_a3.rs` for the white-box key-schedule test.
- The PDF's Appendix C example vectors did not survive text extraction and are not used; the
  NIST core example below supplies complete-cipher evidence instead.

## NIST supplementary AES-256 example

- **Document:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_Core256.pdf>
  (index <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>).
- **Material used:** the ECB-AES256 key (identical to Appendix A.3's) and all four
  plaintext/ciphertext blocks, exercised in both directions through the public API.
- **Conversion:** printed 32-bit groups are joined and lowercased; each block is transformed
  independently ("ECB" names the document's presentation, not an exported mode).

## Differential oracle

RustCrypto `aes` 0.9.2 (`Aes256`), development dependency only.
