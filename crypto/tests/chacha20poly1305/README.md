# AEAD_CHACHA20_POLY1305 public test organization

- `known_answers.rs` contains **published evidence**: RFC 8439 §2.8.2 seal/open, Appendix A.4
  one-time keys, Appendix A.5 decryption, and all 325 Project Wycheproof cases (valid cases are
  also re-sealed byte-exact).
- `boundaries.rs` contains **standard-derived evidence** for per-byte tampering of ciphertext
  and tag, changed nonce/AAD/length, empty inputs, generation, and the generic `Aead` path.
- `differential.rs` contains **differential evidence** against the development-only
  `chacha20poly1305` crate 0.11.0 in both directions.

Provenance: [`../vectors/chacha20-poly1305/README.md`](../vectors/chacha20-poly1305/README.md).
