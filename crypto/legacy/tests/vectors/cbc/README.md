# CBC vector provenance

- **Mode definition:** NIST SP 800-38A, §6.2 and Appendix F:
  <https://csrc.nist.gov/pubs/sp/800/38/a/final>.
- **Official TDES-CBC intermediate values:** NIST `TDES_CBC.pdf`:
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/TDES_CBC.pdf>.
- **Planned standards revision:**
  <https://csrc.nist.gov/news/2023/decision-to-revise-nist-sp-800-38a>.
- **Current TLS deployment guidance:** RFC 9325 §4.2:
  <https://www.rfc-editor.org/rfc/rfc9325.html#section-4.2>.
- **Accessed:** 2026-08-27.

The public known-answer test copies NIST's 24-byte EDE3 key bundle, 8-byte IV, four plaintext
blocks, and four CBC ciphertext blocks. The module's runnable Rustdoc example uses the first
published block. The differential test independently composes RustCrypto `des` 0.9.0 over
deterministic six-block messages and compares every output block and the final chaining value.

No vector is interpreted as evidence for TLS padding, MAC order, record formatting, or oracle-safe
decryption. Those protocol operations are explicitly outside this mode slice.
