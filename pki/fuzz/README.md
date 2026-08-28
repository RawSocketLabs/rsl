# RSL PKI fuzzing

Coverage-guided fuzzing for the three attacker-controlled boundaries in the certificate stack.
This is a separate workspace so `libfuzzer-sys` and the independent comparison parser do not
enter the parent workspace or its stable `Cargo.lock`.

```bash
cargo install cargo-fuzz
cargo +nightly fuzz run der_decode --fuzz-dir pki/fuzz
```

| Target | Boundary | Invariants |
|---|---|---|
| `der_decode` | Arbitrary DER regions | strict decoding and typed accessors never panic; accepted input preserves its exact span and can be copied through the canonical encoder |
| `certificate_parse` | Raw and structured mutations of published NIST PKITS certificates and one guided constructed certificate | parsing never panics; accepted certificates preserve exact certificate and `TBSCertificate` bytes; fields shared with `x509-parser` agree |
| `path_validation` | Structured mutations of a valid Ed25519 chain, decoy anchors, policy inputs, and work budgets | construction and signature/policy checks never panic; the valid seed succeeds; issuer search obeys its configured check budget |

The PKITS fixtures and their hashes are under `../tests/vectors/nist-pkits/`. `x509-parser` is a
development-only independent parser, not part of any RSL crate's dependency graph. Fuzzing is
engineering evidence, not an audit or a security proof.
