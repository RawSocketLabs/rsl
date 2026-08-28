# `rsl-crypto` fuzzing

Coverage-guided fuzzing of every **untrusted-input boundary** in the crate — the contract that
parsers, verifiers, and authenticated decryption fed hostile or garbage bytes return `Ok`/`Err`
but **never panic, read out of bounds, loop unboundedly, or release unauthenticated plaintext** —
plus **consistency invariants** (fragmented input equals one-shot input; seal/open round-trips).

This is a **separate workspace** (note the empty `[workspace]` table in `Cargo.toml`) so the
unstable `libfuzzer-sys` toolchain never touches the parent workspace's stable jobs.

## Run it

```bash
cargo install cargo-fuzz                                              # once
cargo +nightly fuzz run aead_open --fuzz-dir crypto/fuzz              # from the repo root
```

The tree pins stable via `rust-toolchain.toml`, so the explicit `+nightly` is required locally.
A crash drops a reproducer in `crypto/fuzz/artifacts/<target>/`; replay it by passing the path.

## Targets

| Target | Boundary | Invariants |
| --- | --- | --- |
| `aead_open` | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305, and AEAD record opening with arbitrary nonce/AAD/ciphertext/tag/record fields | no panic; genuine one-shot and fragmented-record round-trips; flipped tag fails |
| `signature_verify` | Ed25519, Ed448, ECDSA P-256, ECDSA P-384 key/signature parsing and verification | no panic |
| `public_key_parse` | SEC 1 points (P-256/P-384), Edwards points (Ed25519/Ed448), Montgomery coordinates (X25519/X448) and agreement over them | no panic |
| `digest_fragmentation` | SHA-256/384/512, SHA3-256, SHAKE256, HMAC, Poly1305, and HKDF with arbitrary fragmentation | fragmented equals one-shot |
| `rsa_pss_verify` | RSA component import and RSASSA-PSS verification with arbitrary moduli, exponents, and signatures | no panic, bounded time |

Fuzzing is engineering evidence, not a security proof: it does not observe timing, and it cannot
show that an accepted input was cryptographically correct. Published vectors, Wycheproof suites,
and differential tests in `crypto/tests/` cover correctness.
