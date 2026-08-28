# rsl-pki

Trust-anchor, path-construction, constraint, and certificate-signature validation for RSL.

```rust,ignore
use rsl_pki::{PathValidator, Purpose, RequiredKeyUsage};
use rsl_x509::{Certificate, Time};

let leaf = Certificate::from_der(leaf_der)?;
let intermediates = [Certificate::from_der(intermediate_der)?];
let anchors = [Certificate::from_der(root_der)?];
let now = Time::new(2026, 8, 28, 12, 0, 0)?;

let path = PathValidator::builder()
    .leaf(&leaf)
    .intermediates(&intermediates)
    .trust_anchors(&anchors)
    .at_time(now)
    .purpose(Purpose::ServerAuth)
    .required_key_usage(RequiredKeyUsage::DigitalSignature)
    .dns_name("www.example.test")
    .validate()?;
```

The three required setters replace compile-time marker types. `validate()` exists only for
`HasLeaf + HasAnchors + HasTime`; optional policy setters remain available in any order. This
prevents accidentally attempting validation without its required external inputs while keeping
those inputs explicit.

Implemented policy is deliberately narrower than all of RFC 5280: unsupported critical
extensions fail closed; policy-tree, name-constraints, root-store loading, clocks, and revocation
transport are not inferred. Candidate paths use conservative exact-DER name matching.

This implementation is unaudited. It makes no production-security claim.
