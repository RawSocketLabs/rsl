# RSL PKI crates

> Inherits the workspace-root `../AGENTS.md`. `CLAUDE.md` is a symlink to this file. This is the
> single guide for all three crates (`asn1`, `x509`, `pki`); they carry no per-crate `AGENTS.md`.

- `asn1/` owns strict DER transport mechanics and uses `bitsandbytes` directly.
- `x509/` owns certificate syntax, canonical construction, and exact signed-byte preservation.
- `validation/` owns supported signing-key adapters, trust, path construction, constraints, and
  signature verification. CA authorization, serial allocation, and private-key custody remain
  caller policy.
- Decode attacker-controlled lengths without preallocating from them.
- Reject non-canonical DER; do not silently normalize signed input.
- Preserve the exact `TBSCertificate` encoding used for signature verification.
- Unknown critical extensions fail validation closed on every non-anchor certificate.
- Protocol transcript binding, certificate negotiation, root-store integration, clocks, and
  revocation transport remain outside these crates.
- Every standards-derived rule is traced in the crate's `STANDARDS.md`.

## Layout beyond the crates

- `fuzz/` is a detached cargo-fuzz workspace (own `[workspace]` and `Cargo.lock`, edition 2024,
  nightly). See `fuzz/README.md`.
- `tests/vectors/` holds the shared NIST PKITS fixtures and their provenance.

## Verify

Routine:

```sh
cargo fmt --all --check
cargo test -p asn1 -p x509 -p pki
cargo clippy -p <crate> --all-targets -- -D warnings
```

Changes under `tests/vectors/` select all three crates plus the `pki/fuzz` workspace. Fuzz
campaigns are release tier: `cargo +nightly fuzz run <target> --fuzz-dir pki/fuzz` (CI runs each
target for 45 s).
