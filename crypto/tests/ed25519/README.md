# Ed25519 public evidence

- `known_answers.rs` reproduces RFC 8032 §7.1 tests 1–3 (pure), §7.2's four Ed25519ctx vectors,
  and §7.3's Ed25519ph vector through public key derivation, signing, and verification.
- `boundaries.rs` checks exact wire lengths, canonical `S`, strict point behavior, changed inputs,
  context length limits, cross-variant rejection, deterministic signing, generic traits, and
  caller-owned entropy.
- `differential.rs` compares 32 deterministic pure key/message cases with `ed25519-dalek` 3.0.0,
  requiring both strict verification paths to accept the same signatures, and 32 Ed25519ph cases
  (with and without a context) against its prehashed path.

Private formula and representation tests remain beside their owning source layers.
