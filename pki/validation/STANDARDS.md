# Standards traceability

Accessed 2026-08-28.

| Publication | Revision and status | Authoritative source | Owned rules |
|---|---|---|---|
| RFC 5280 | May 2008, Standards Track; obsoletes RFC 3280 | <https://www.rfc-editor.org/rfc/rfc5280> | §4.2 critical-extension, basic-constraints, key-usage and EKU semantics; §6 path validation inputs and basic processing |
| RFC 9525 | November 2023, Standards Track; obsoletes RFC 6125 | <https://www.rfc-editor.org/rfc/rfc9525> | §6 DNS-ID matching; no common-name fallback; single complete left-most wildcard label |
| RFC 8017 | November 2016, Informational | <https://www.rfc-editor.org/rfc/rfc8017> | §8.1 RSASSA-PSS verification, delegated to `rsl-crypto` |

This profile does not implement the RFC 5280 certificate-policy tree, policy mapping, or name
constraints. A critical extension outside the explicit supported set is rejected. Revocation
evidence is supplied through `RevocationChecker`; fetching and freshness policy are external.
The 1,024-check default path-search budget is an implementation resource limit, not a rule from
RFC 5280; callers may select a different explicit budget.

Tests create standard-derived local Ed25519 certificates and are not published vectors.

`issuance` implements no new signature primitive. It maps `rsl-crypto`'s pure Ed25519/Ed448 and
ECDSA P-256/SHA-256/P-384/SHA-384 signing operations to the exact RFC 8410/RFC 5480 identifiers and
X.509 ECDSA signature framing owned by `rsl-x509`. Standard-derived tests construct and verify all
four profiles, validate a guided CA/leaf chain, and prove that a raw critical extension remains
fail-closed. The detached parser fuzz target compares constructed certificates with an independent
X.509 parser. These are engineering tests, not published certificate vectors or audit evidence.
