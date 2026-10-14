# GHASH evidence provenance

## Controlling definition

- **Publication:** NIST SP 800-38D, *Recommendation for Block Cipher Modes of Operation:
  Galois/Counter Mode (GCM) and GMAC*.
- **Revision:** November 2007 final publication.
- **Publication record:** <https://csrc.nist.gov/pubs/sp/800/38/d/final>.
- **Persistent identifier:** <https://doi.org/10.6028/NIST.SP.800-38D>.
- **Authoritative document:**
  <https://nvlpubs.nist.gov/nistpubs/legacy/sp/nistspecialpublication800-38d.pdf>.
- **Material used:** §6.3's reduction block, displayed-bit/polynomial convention, and complete
  Algorithm 1 multiplication procedure; §6.4's complete-block Algorithm 2 recurrence.
- **Checked:** 2026-08-27.

The final 2007 publication remains controlling. NIST's June 1, 2026 second preliminary call for
comments on Revision 1 explicitly says that no draft document is yet available. It discusses a
possible 256-bit wide-GHASH for a future wGCM variant; it does not replace the current 128-bit
GHASH definition.

## Supplementary NIST example

- **Evidence class:** published operands with an explicitly standard-derived intermediate result.
- **Publisher:** National Institute of Standards and Technology.
- **Index:**
  <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>.
- **Document:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_GCM.pdf>.
- **Case used:** GCM-AES128 Example 2 publishes hash subkey
  `b83b533708bf535d0aa6e52980d53b78` and first ciphertext block
  `42831ec2217774244b7221b784d0d49c`. It also publishes all four ciphertext blocks and the final
  GHASH result `S = 7f1b32b81b820d02614f8895ac1d4eac`.
- **Checked:** 2026-08-27.
- **Conversion policy:** spaces and line wrapping are removed; each hexadecimal pair becomes one
  byte without reversing the displayed byte or bit order.

Applying SP 800-38D §6.3 Algorithm 1 to those two published operands gives
`59ed3f2bb1a0aaa07c9f56c6a504647b`. NIST does **not** print that individual product in the
supplementary example, so the test labels it standard-derived rather than a published
known-answer value. The complete Algorithm 2 test supplies the four ciphertext blocks followed by
SP 800-38D Algorithm 4's `[0]_64 || [512]_64` length block and compares the result directly with
the document's published `S`; that final comparison is published known-answer evidence without
changing the classification of the individual intermediate product.

## Differential oracle

- **Evidence class:** differential comparison, not published-vector evidence.
- **Implementation:** RustCrypto `ghash` 0.6.0, development dependency only.
- **Project:** <https://github.com/RustCrypto/universal-hashes>.
- **API documentation:** <https://docs.rs/ghash/0.6.0/ghash/>.
- **Checked:** 2026-08-27.

The differential test compares the complete-block Algorithm 2 result for 32 deterministic hash
subkeys and sequences containing one through eight blocks. RustCrypto internally expresses GHASH
through POLYVAL, making it structurally independent from this repository's direct transcription of
SP 800-38D Algorithm 1. It is neither a production dependency nor a standards authority.
