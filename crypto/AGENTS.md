# rsl-crypto

> Inherits the workspace-root `../AGENTS.md`.

An accuracy-first cryptographic library. The implementation should read like an executable
specification: preserve named intermediate values, link tests to published vectors, and prefer
obvious operations over table tricks, SIMD, assembly, or clever fusion.

## Rules

- `#![forbid(unsafe_code)]`, `#![no_std]`, `alloc` available.
- Secret-bearing types are non-`Clone`, redact `Debug`, and zeroize on drop.
- Authenticated decryption never returns plaintext before tag verification.
- Keys, nonces, counters, tags, and digests use distinct types in concrete algorithms.
- Protocol constructions (TLS records, SSH packets) live in protocol crates; this crate supplies
  primitives and narrowly defined primitive contracts.
- Every algorithm needs published known-answer tests, intermediate-state tests where vectors
  provide them, negative tests, and differential tests before it is advertised as usable.
- `STANDARDS.md` is the traceability ledger. Update it in the same change that implements or
  removes a standards-defined operation, constant, encoding rule, or state transition.
- Every standards-derived module names the exact controlling publication and the sections it
  owns. Every derived item names its equation, table, section, or algorithm step when one exists.
- Explain how standards notation maps to Rust operations whenever representation, byte order,
  overflow, rotation, shifting, or indexing affects correctness.
- Label test evidence as published, standard-derived, regression, or differential. Never present
  a locally calculated intermediate value as a vector published by the standard.
- Store the publication revision, authoritative link, access date, and any supersession notice in
  `STANDARDS.md`; store imported vector provenance and conversions under `tests/vectors/`.
- Performance work must not obscure the readable reference path. Add an optimized implementation
  beside it rather than rewriting away the reference implementation.
- No production-security claim without side-channel review, fuzzing, differential testing, and
  an independent cryptographic audit.
