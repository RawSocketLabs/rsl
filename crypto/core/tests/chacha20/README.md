# ChaCha20 public test organization

- `known_answers.rs` contains **published evidence** from RFC 8439 Appendix A.1 (block
  functions) and A.2 (encryptions, both directions), plus **standard-derived** counter-wrap and
  nonce-length boundaries.
- Body-example intermediates (§2.1.1, §2.2.1, §2.3.2, §2.4.2) are white-box tests beside the
  implementation.

Provenance: [`../vectors/chacha20-poly1305/README.md`](../vectors/chacha20-poly1305/README.md).
