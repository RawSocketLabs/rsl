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

Tests create standard-derived local Ed25519 certificates and are not published vectors.
