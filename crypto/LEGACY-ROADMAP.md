# Historical cryptography sequence

Historical interoperability is built only after the contemporary SHA-256/HMAC/HKDF/AES-GCM,
X25519, SHA-512, and Ed25519 reference paths establish the API and evidence pattern.

1. Define the shared `SecurityStatus` taxonomy and the no-implicit-fallback rule. **Complete.**
2. Scaffold `rsl-crypto-legacy` as a separate package in this repository. **Next.**
3. Implement SHA-1 and MD5 with historical standards, published vectors, collision/security
   notices, boundary tests, and independent differential evidence.
4. Implement RC4, then DES and Triple-DES, with exact key/profile types and deprecation evidence.
5. Implement the CBC-era primitive constructions required for historical TLS/SSH while keeping
   padding, MAC ordering, record splitting, IV derivation, and oracle behavior protocol-owned.
6. Implement RSA PKCS #1 v1.5 encryption/signature primitives needed by the explicitly selected
   historical protocol profiles.

Nothing in this roadmap authorizes default negotiation. A protocol must name a historical suite,
enable the legacy dependency, and opt into an explicit allowlist. Broken algorithms remain useful
for decoding captures, interoperability labs, and teaching attacks; correctness never upgrades
their security status.
