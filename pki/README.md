# RSL certificate stack

X.509 is a layer above cryptographic primitives and below protocol state. The stack is split so
DER transport, certificate meaning, and trust policy can evolve independently.

| Crate | Owns | Direct foundation |
|---|---|---|
| [`rsl-asn1`](asn1/) | Strict DER tag/length/value transport and canonical encoding | `bitsandbytes` |
| [`rsl-x509`](x509/) | Borrowed certificate fields, algorithms, keys, extensions, and exact `TBSCertificate` bytes | `rsl-asn1` |
| [`rsl-pki`](validation/) | Trust anchors, path construction, signatures, constraints, purpose, and service identity | `rsl-x509` + `rsl-crypto` |

Only the transport layer uses `bitsandbytes` directly. Higher layers still benefit from its exact
borrowed spans and bounded cursors without turning the codec into a certificate-policy library.
TLS/SSH certificate negotiation, transcript binding, clocks, root-store loading, replay, and
revocation transport remain protocol or platform responsibilities.

All three crates are `no_std + alloc`, forbid unsafe code, and are unaudited. They make no
production-security claim.
