# SHA-384 vector provenance

- **Controlling publication:** NIST FIPS 180-4, *Secure Hash Standard*, August 2015, §5.3.4 and
  §6.5 (with the SHA-512 sections). <https://doi.org/10.6028/NIST.FIPS.180-4>. Accessed 2026-08-28.

## NIST intermediate-value example

- **Document:** NIST, *Secure Hash Algorithm — Message Digest Length = 384*,
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA384.pdf>.
- **Cases:** "One Block Message Sample" (`"abc"`) and "Two Block Message Sample" (the 112-byte
  message). The public test uses the printed digests; the white-box test in
  `src/digest/sha2/sha384/state.rs` uses all eight final `H` words the document prints,
  including `H[6]` and `H[7]`, which SHA-384 discards.
- **Conversion:** spaces between printed digest words removed, hexadecimal lowercased.

## NIST CAVP byte-oriented vectors

- **Archive:** `shabytetestvectors.zip` from
  <https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/secure-hashing>.
- **SHA-256 of archive as downloaded 2026-08-28:**
  `929ef80b7b3418aca026643f6f248815913b60e01741a44bba9e118067f4c9b8`.
- **File and cases:** `SHA384ShortMsg.rsp`, CAVS 11.0; `Len = 0, 8, 888, 896, 1016, 1024` bits,
  which bracket the 111/112-byte padding threshold and the 128-byte block boundary.

`sha2::Sha384` 0.11 is a development-only differential oracle.
