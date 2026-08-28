# RSL PKI crates

> Inherits the workspace-root `../AGENTS.md`.

- `asn1/` owns strict DER transport mechanics and uses `bitsandbytes` directly.
- `x509/` owns certificate syntax and exact signed-byte preservation.
- `validation/` owns trust, path construction, constraints, and signature verification.
- Decode attacker-controlled lengths without preallocating from them.
- Reject non-canonical DER; do not silently normalize signed input.
- Preserve the exact `TBSCertificate` encoding used for signature verification.
- Unknown critical extensions fail validation closed.
- Protocol transcript binding, certificate negotiation, root-store integration, clocks, and
  revocation transport remain outside these crates.
- Every standards-derived rule is traced in the crate's `STANDARDS.md`.
