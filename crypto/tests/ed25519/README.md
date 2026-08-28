# Ed25519 public evidence

- `known_answers.rs` reproduces RFC 8032 §7.1 tests 1–3 through public key derivation, signing,
  and verification.
- `boundaries.rs` checks exact wire lengths, canonical `S`, strict point behavior, changed inputs,
  deterministic signing, generic traits, and caller-owned entropy.
- `differential.rs` compares 32 deterministic key/message cases with `ed25519-dalek` 3.0.0 and
  requires both implementations' strict verification paths to accept the same signatures.

Private formula and representation tests remain beside their owning source layers.
