# rsl-x509

Typed, borrowed X.509 certificate structures with exact `TBSCertificate` preservation.

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

This implementation is unaudited. It makes no production-security claim.
