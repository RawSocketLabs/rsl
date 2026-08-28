# RSL certificate stack

X.509 is a layer above cryptographic primitives and below protocol state. The stack is split so
DER transport, certificate meaning, and trust policy can evolve independently.

| Crate | Owns | Direct foundation |
|---|---|---|
| [`rsl-asn1`](asn1/) | Strict DER tag/length/value transport and canonical encoding | `bitsandbytes` |
| [`rsl-x509`](x509/) | Borrowed certificate fields, canonical construction, extensions, and exact `TBSCertificate` bytes | `rsl-asn1` |
| [`rsl-pki`](validation/) | Signing-key adapters; trust anchors, path construction, constraints, purpose, and service identity | `rsl-x509` + `rsl-crypto` |

Only the transport layer uses `bitsandbytes` directly. Higher layers still benefit from its exact
borrowed spans and bounded cursors without turning the codec into a certificate-policy library.
TLS/SSH certificate negotiation, transcript binding, clocks, root-store loading, replay, and
revocation transport remain protocol or platform responsibilities.

[`DESIGN.md`](DESIGN.md) defines the guided-path/escape-hatch contract. Normal trust decisions use
the typestate validator; exact DER spans, raw extension values, individual signature checks, and
configurable path-search budgets remain available for interoperability work and experiments
without being mislabeled as validated trust.

Certificate construction follows the same split. Guided end-entity and CA typestates live in
`rsl-x509`; `rsl-pki::issuance` adapts supported signing keys. Raw canonical names, SPKIs,
extensions, algorithms, and staged external signing remain available without turning a constructed
certificate into trusted state.

[`fuzz/`](fuzz/README.md) covers strict DER, differential certificate parsing against an
independent implementation, and structured path-validation mutations. Published NIST PKITS
fixtures and byte-level provenance live under [`tests/vectors/`](tests/vectors/).

All three crates are `no_std + alloc`, forbid unsafe code, and are unaudited. They make no
production-security claim.
