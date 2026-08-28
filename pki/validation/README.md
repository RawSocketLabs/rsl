# rsl-pki

Trust-anchor, path-construction, constraint, and certificate-signature validation for RSL.

`rsl-pki::issuance` also supplies the narrow adapter between `rsl-x509`'s syntax builder and the
Ed25519, Ed448, ECDSA P-256, and ECDSA P-384 signing keys in `rsl-crypto`:

```rust,ignore
use rsl_crypto::signature::ed25519::Ed25519SigningKey;
use rsl_pki::issuance::Ed25519CertificateSigner;
use rsl_x509::{Time, builder::{CertificateBuilder, NameDer}};

let key = Ed25519SigningKey::from_seed(seed);
let signer = Ed25519CertificateSigner::new(&key);
let name = NameDer::common_name("Example root")?;
let root_der = CertificateBuilder::certificate_authority(
        &[1], name.clone(), name, Some(0),
    )?
    .validity(
        Time::new(2026, 1, 1, 0, 0, 0)?,
        Time::new(2027, 1, 1, 0, 0, 0)?,
    )?
    .subject_public_key_info(signer.subject_public_key_info()?)
    .sign(&signer)?;
```

This convenience produces canonical certificate syntax, not issuance authorization or trust.
Serial allocation, permitted names, lifetimes, CA operation, and private-key custody remain
caller-owned policy. Generic and staged signer contracts remain available for hardware, remote,
test, and experimental signers.

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
    .max_candidate_checks(1_024)
    .validate()?;
```

The three required setters replace compile-time marker types. `validate()` exists only for
`HasLeaf + HasAnchors + HasTime`; optional policy setters remain available in any order. This
prevents accidentally attempting validation without its required external inputs while keeping
those inputs explicit.

Implemented policy is deliberately narrower than all of RFC 5280: unsupported critical
extensions fail closed; policy-tree, name-constraints, root-store loading, clocks, and revocation
transport are not inferred. Candidate paths use conservative exact-DER name matching.

Path construction defaults to at most 1,024 issuer-candidate checks, independently of the number
of untrusted intermediates supplied by a peer. `max_candidate_checks` selects an explicit CPU-work
budget; exhausting it fails closed.

This implementation is unaudited. It makes no production-security claim.
