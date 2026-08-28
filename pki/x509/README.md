# rsl-x509

Typed, borrowed X.509 certificate structures with exact `TBSCertificate` preservation, plus
canonical certificate construction.

```rust,ignore
use rsl_x509::Certificate;

let certificate = Certificate::from_der(certificate_der)?;
let signed_bytes = certificate.tbs_certificate().encoded();
let issuer = certificate.tbs_certificate().issuer();
let public_key = certificate
    .tbs_certificate()
    .subject_public_key_info()
    .public_key()?;
```

The parser owns certificate syntax, algorithm identifiers, names, validity, public keys, and the
extension encodings needed by `rsl-pki`. It does not choose trust anchors or claim that a parsed
certificate is trusted. Verification must use `TbsCertificate::encoded()` directly; parsing never
re-encodes signed input.

Construction follows a typestate path:

```rust,ignore
use rsl_x509::{Time, builder::{CertificateBuilder, NameDer, SubjectPublicKeyInfoDer}};

let issuer = NameDer::common_name("Example issuer")?;
let subject = NameDer::common_name("service.example")?;
let ready = CertificateBuilder::end_entity(&[1], issuer, subject)?
    .validity(
        Time::new(2026, 1, 1, 0, 0, 0)?,
        Time::new(2027, 1, 1, 0, 0, 0)?,
    )?
    .subject_public_key_info(SubjectPublicKeyInfoDer::ed25519(public_key)?);

let certificate_der = ready.sign(&certificate_signer)?;
```

The guided end-entity and CA constructors install conservative basic-constraints and key-usage
extensions. `custom`, `NameDer::from_der`, `SubjectPublicKeyInfoDer::from_der`,
`ExtensionDer::from_parts`, `raw_extension`, and staged `build_tbs`/`finish` are explicit syntax
escape hatches. They do not mark the result as issued correctly or trusted. Built-in `rsl-crypto`
signing-key adapters live in `rsl-pki::issuance` so this syntax crate remains independent of
secret-key implementations.

Parser interoperability tests include the published NIST PKITS §4.1.1 root, intermediate, and
end-entity certificates. The detached fuzz workspace also parses constructed certificates with
the independent `x509-parser` implementation. PKITS provenance and byte hashes are recorded under
`../tests/vectors/nist-pkits/`.

This implementation is unaudited. It makes no production-security claim.
