# Standards traceability

Accessed 2026-08-28.

| Publication | Revision and status | Authoritative source | Owned rules |
|---|---|---|---|
| RFC 5280 | May 2008, Standards Track; obsoletes RFC 3280 | <https://www.rfc-editor.org/rfc/rfc5280> | §4 certificate/profile syntax, names, validity, SPKI, and extensions; Appendix A ASN.1 |
| RFC 5480 | March 2009, Standards Track | <https://www.rfc-editor.org/rfc/rfc5480> | §2 EC `SubjectPublicKeyInfo`; §2.1 named-curve identifiers |
| RFC 8410 | August 2018, Standards Track | <https://www.rfc-editor.org/rfc/rfc8410> | §§3–4 Ed25519/Ed448 algorithm identifiers and public-key encoding |
| RFC 4055 | June 2005, Standards Track | <https://www.rfc-editor.org/rfc/rfc4055> | §§2–3 RSASSA-PSS algorithm identifiers and parameters |
| NIST PKITS | Version 1.0.1, April 2011 | <https://csrc.nist.gov/Projects/pki-testing> | Imported parser-interoperability certificates from §4.1.1 Valid Signatures Test1; provenance and byte hashes are under `../tests/vectors/nist-pkits/` |

The compact unit-test certificate is standard-derived local evidence, not a published vector.
The three PKITS certificates are published external parser fixtures; their RSA-with-SHA-256
signatures are outside the current validation profile.

## Certificate construction coverage

| Rule | Construction surface and evidence | Status |
|---|---|---|
| RFC 5280 §§4.1.1–4.1.2 | `builder::CertificateBuilder` emits V3 `TBSCertificate`, repeats one exact signer-selected `AlgorithmIdentifier`, signs the exact complete TBS element, and assembles an unused-bit-free signature bit string. Tests parse every constructed result and verify every built-in signature. | Implemented for the builder profile. |
| RFC 5280 §4.1.2.2 | Serial input is an explicit positive unsigned magnitude of at most 20 octets; zero, empty, overlong, and non-canonical leading-zero inputs are rejected before encoding. | Implemented and negatively tested. |
| RFC 5280 §4.1.2.5 | Validity uses `UTCTime` for 1950–2049 and `GeneralizedTime` for 2050–9999, always in `Z` form with seconds; reversed/out-of-range intervals are rejected. | Implemented and tested through parse round trips. |
| RFC 5280 §§4.1.2.4, 4.1.2.6, Appendix A | `NameBuilder` emits ordered single-valued RDNs; `NameDer::from_der` preserves any strict-DER name for explicit advanced use. Guided issuers and CA subjects are non-empty. | Implemented; multi-valued RDN construction uses the raw-DER import boundary. |
| RFC 5280 §§4.2.1.1–4.2.1.2, 4.2.1.3, 4.2.1.6, 4.2.1.9, 4.2.1.12 | Typed SKI, AKI-key-id, key-usage, SAN, basic-constraints, and EKU builders; guided CA/end-entity defaults; duplicate OIDs rejected. Empty end-entity subjects require a critical SAN. | Implemented and tested. |
| RFC 5480 §2; RFC 8410 §§3–4 | Typed SPKI constructors cover uncompressed P-256/P-384 and parameter-absent Ed25519/Ed448. `SubjectPublicKeyInfoDer::from_der` is the strict-syntax escape hatch for other algorithms. | Implemented and exercised by all four signing adapters. |
| Caller-governed extensions and signers | `ExtensionDer::from_parts`, `raw_extension`, `SignatureAlgorithmDer::from_der`, `CertificateSigner`, and staged `build_tbs`/`finish` preserve canonical syntax without claiming supported semantics or trust. A critical unknown extension remains rejected by `rsl-pki`. | Implemented and fail-closed tested. |
