# AES-128 public test organization

These tests exercise only the exported `Aes128`, `Aes128Key`, and `Aes128Block` boundary. Private
field, substitution, state-transform, and key-schedule evidence remains beside the implementation
so a reviewer can compare each layer directly with FIPS 197-upd1.

- `known_answers.rs` contains **published evidence** from FIPS 197-upd1 Appendix B and all four
  blocks in NIST's supplementary `AES_Core128.pdf`, in both directions.
- `round_trip.rs` contains **local regression evidence** over varied deterministic keys and blocks,
  plus a generic `BlockCipher` contract check. A round trip alone cannot prove interoperability.
- `differential.rs` contains **differential evidence** against the development-only RustCrypto
  `aes` implementation over 192 deterministic key/block pairs in both directions.

Exact published-vector and independent-oracle provenance lives in
[`../vectors/aes-128/README.md`](../vectors/aes-128/README.md). Passing these tests is not NIST
validation, an audit, or a side-channel-resistance claim.
