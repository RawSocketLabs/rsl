# rsl-crypto-legacy

Explicitly opt-in historical and broken cryptographic primitives for interoperability, capture
decoding, test fixtures, and education.

This package is intentionally absent from the `rsl` facade's `full` and `transforms` bundles. A
consumer must request the `legacy-crypto` feature or depend on this crate directly. Protocols must
then name an explicit historical allowlist; there is no automatic fallback.

The package establishes the isolation, documentation, standards, and security-status boundaries.
Its functional primitives are SHA-1, MD5, RC4, DES, two-key/three-key Triple-DES EDE, generic
complete-block CBC chaining, and RSA PKCS #1 v1.5 encryption plus SHA-1/SHA-256 signatures. Each
reproduces published historical output, carries a machine-readable non-recommended status, and
has published, boundary, intermediate or state, malformed-input, and independent differential
evidence. Exact source coverage is recorded in `STANDARDS.md`.

```rust
use rsl_crypto_legacy::{SecurityStatus, digest::sha1};

assert_eq!(sha1::SECURITY_STATUS, SecurityStatus::Broken);
let digest = sha1::Sha1::digest("historical bytes")?;
assert_eq!(digest.as_bytes().len(), 20);
# Ok::<(), rsl_crypto_legacy::CryptoError>(())
```

RC4 is likewise named directly and stateful; no protocol can acquire it through a generic
fallback. Its module-level Rust documentation walks through the KSA, PRGA, bidirectional XOR, and
historical SSH discard behavior while keeping modern TLS/SSH prohibitions prominent.

DES and Triple-DES expose only typed block permutations. CBC is a separate stateful layer generic
over that block-cipher contract. It accepts complete blocks and intentionally has no padding, MAC,
record, or negotiation behavior.

RSA imports raw unsigned `n`, `e`, and `d` components so the integer primitive and PKCS #1
encoding remain visible. It intentionally has no key generation, CRT, ASN.1, certificate, TLS, or
SSH behavior. Its private operation is variable-time and unblinded; a uniform decryption error is
not a claim of padding-oracle resistance.

All package statuses describe algorithm lifecycle, not audit readiness. Nothing in this package
is recommended for new protection.
