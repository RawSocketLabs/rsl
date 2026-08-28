# NIST PKITS 1.0.1 certificate fixtures

Accessed 2026-08-28 from the authoritative NIST PKI Testing download:
<https://csrc.nist.gov/CSRC/media/Projects/PKI-Testing/documents/PKITS_data.zip>.
The suite description is <https://csrc.nist.gov/CSRC/media/Projects/PKI-Testing/documents/pkits.pdf>.

These are the three certificates for PKITS §4.1.1, Valid Signatures Test1. The `.hex` files are
lowercase hexadecimal renderings of the original DER bytes; tests remove ASCII whitespace and
decode pairs of digits without otherwise transforming the certificate.

| Local file | Original archive path | DER SHA-256 |
|---|---|---|
| `TrustAnchorRootCertificate.hex` | `certs/TrustAnchorRootCertificate.crt` | `87d1dfcc73f979bb348bb4f159d9115c40ab0a9afc4b21d77e6ddf20c7782b89` |
| `GoodCACert.hex` | `certs/GoodCACert.crt` | `86d218374763fce77d5b2b45398db48f10e553da1875be7d6103085baca0343f` |
| `ValidCertificatePathTest1EE.hex` | `certs/ValidCertificatePathTest1EE.crt` | `967ed7ed2be0506b82000a377751c5525619d3b9e7fed8a0e7aa554947af5e9e` |

PKITS uses algorithms and policy extensions outside the current `rsl-pki` validation profile.
These fixtures therefore provide independent certificate-parser interoperability evidence; they
are not represented as a successful `rsl-pki` path-validation vector.
