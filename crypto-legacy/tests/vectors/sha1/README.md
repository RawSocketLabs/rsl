# SHA-1 vector provenance

- **Algorithm source:** NIST FIPS 180-4, *Secure Hash Standard*, §§5.3.1 and 6.1.
- **Record:** <https://csrc.nist.gov/pubs/fips/180-4/upd1/final>.
- **Security transition:** <https://csrc.nist.gov/News/2022/nist-transitioning-away-from-sha-1-for-all-apps>.
- **Accessed:** 2026-08-27.

The public tests copy FIPS's `abc` and two-block messages and their complete 20-byte digests.
RustCrypto `sha1` 0.11.0 is a development-only differential oracle. Practical collision evidence
is recorded in `STANDARDS.md`; collision fixtures are not required to execute the historical hash.
