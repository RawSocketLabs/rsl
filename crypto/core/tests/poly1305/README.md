# Poly1305 public test organization

- `known_answers.rs` contains **published evidence** from RFC 8439 Appendix A.3 (all eleven
  vectors, including the `r = 0` weak key and seven reduction edge cases) and the §2.5.2 tag
  through the generic `Mac` contract, plus key/tag length boundaries.
- §2.5.2's clamped `r`, `s`, and every intermediate accumulator value are white-box tests.

Provenance: [`../vectors/chacha20-poly1305/README.md`](../vectors/chacha20-poly1305/README.md).
