# Historical primitive order

1. Isolation, opt-in facade wiring, and security taxonomy. **Implemented.**
2. SHA-1. **Implemented with FIPS vectors and differential evidence.**
3. MD5. **Implemented with all RFC 1321 Appendix A.5 vectors and differential evidence.**
4. RC4. **Implemented with RFC 6229 offset vectors and differential evidence.**
5. DES and Triple-DES. **Implemented with official NIST intermediate values and differential evidence.**
6. Narrow CBC primitive profiles. **Implemented as generic complete-block chaining with NIST vectors.**
7. RSA PKCS #1 v1.5 historical encryption and SHA-1/SHA-256 signature profiles. **Implemented
   with NIST CAVP, Wycheproof, boundary, malformed-input, and differential evidence.**

Protocol cipher-suite work starts only after the required primitives are implemented and remains
in the TLS/SSH repository. It is never pulled into this crate for convenience.
