# SHA-512 vector provenance

- **Controlling publication:** NIST FIPS 180-4, *Secure Hash Standard*, August 2015.
- **Authoritative record:** <https://csrc.nist.gov/pubs/fips/180-4/upd1/final>.
- **Publication DOI:** <https://doi.org/10.6028/NIST.FIPS.180-4>.
- **Accessed:** 2026-08-27.
- **Relevant locations:** §§5.1.2, 5.2.2, 5.3.5, and 6.4, plus the SHA-512 examples.

The public known-answer test copies the `abc` and 112-byte SHA-512 messages and their complete
64-byte digests. Whitespace in the printed hexadecimal is removed without changing byte order.
The differential test uses RustCrypto `sha2` only as a development dependency; it is independent
evidence and is not part of the `rsl-crypto` implementation.
