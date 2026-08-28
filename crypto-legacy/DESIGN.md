# rsl-crypto-legacy design

## Boundary

This crate owns obsolete primitive transformations. `rsl-crypto` owns contemporary primitives and
shared byte/error/security-status contracts. Protocol crates own cipher suites, versions,
negotiation, transcript/record state, padding rules, MAC composition, alerts, and downgrade policy.

## Isolation invariants

- Separate Cargo package and facade feature.
- Not included by `rsl/full`, `rsl/transforms`, or any default feature.
- No algorithm may carry `SecurityStatus::Recommended`.
- No generic “best cipher,” fallback, or negotiation API.
- Historical encryption remains possible only through an explicitly named algorithm API, because
  exact bidirectional reproduction can be necessary for labs and interoperability.

## Reference implementation policy

The same executable-specification policy as `rsl-crypto` applies: named intermediate values,
obvious arithmetic, exact source mapping, published vectors, and independent differential tests.
Security-failure demonstrations supplement correctness evidence; they do not replace it.

## RSA-specific boundary

The crate imports raw unsigned RSA components and owns only the integer primitive and explicitly
named PKCS #1 v1.5 encodings. Key generation, primality validation, CRT acceleration, ASN.1,
certificates, SSH key blobs, TLS premaster-secret handling, and negotiation remain outside this
crate. The teaching integer engine is variable-time and unblinded, so a uniform padding-error enum
must not be represented as complete oracle resistance.
