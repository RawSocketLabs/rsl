# rsl-crypto repository structure

The crate is organized by cryptographic primitive, then by specification layer. Public modules
identify algorithm families. Private modules hold the small operations needed to explain and
validate one algorithm.

```text
rsl-crypto/
├── src/
│   ├── digest/
│   │   ├── mod.rs                 digest contract
│   │   └── sha2/
│   │       ├── mod.rs             SHA-2 family boundary
│   │       ├── sha256/
│   │           ├── mod.rs         public SHA-256 surface
│   │           ├── constants.rs   tested initial words and round constants
│   │           ├── functions.rs   tested Ch, Maj, and sigma functions
│   │           ├── schedule.rs    tested parsing and complete 64-word schedule
│   │           ├── compression.rs 64 rounds and feed-forward
│   │           └── state.rs       streaming, length, padding, and output
│   │       ├── sha512/             independent 64-bit SHA-2 specification layers
│   │       └── sha384/             SHA-384 initial words and truncated state over SHA-512 layers
│   ├── mac.rs                     generic MAC contract and family boundary
│   ├── mac/hmac/sha256/           HMAC-SHA-256 key and state layers
│   ├── mac/hmac/sha384/           HMAC-SHA-384 key and state layers (B = 128, L = 48)
│   ├── kdf.rs                     generic key-expansion contract and family boundary
│   ├── kdf/hkdf/sha256/           HKDF-SHA-256 Extract/Expand layers
│   ├── kdf/hkdf/sha384/           HKDF-SHA-384 Extract/Expand layers (HashLen = 48)
│   ├── cipher.rs                  block and stream cipher contracts
│   ├── cipher/aes/aes128/         public AES-128 boundary over private specification layers
│   ├── cipher/chacha20/quarter_round.rs RFC 8439 §2.1–§2.2 quarter round
│   ├── cipher/chacha20/block.rs   §2.3 state and block function
│   ├── cipher/chacha20/api.rs     typed key/nonce, §2.4 keystream, stateful stream contract
│   ├── mac/poly1305/key.rs        §2.5 `r` clamping and `s`
│   ├── mac/poly1305/state.rs      §2.5.1 accumulator modulo `2^130 - 5`
│   ├── mac/poly1305/api.rs        typed one-time key/tag, buffering, `Mac` contract
│   ├── aead.rs                    authenticated-encryption contract and mode boundary
│   ├── aead/chacha20poly1305/construction.rs §2.6 key derivation and §2.8 composition
│   ├── aead/chacha20poly1305/limits.rs counter-derived payload limit
│   ├── aead/chacha20poly1305/api.rs typed AEAD key/nonce/tag and `Aead` contract
│   ├── aead/gcm/api.rs            public typed AES-128-GCM profile
│   ├── aead/gcm/counter.rs        private SP 800-38D `inc32` counter operation
│   ├── aead/gcm/gctr.rs           private byte-aligned GCTR composition
│   ├── aead/gcm/setup.rs          private hash-subkey and 96-bit-IV setup
│   ├── aead/gcm/authentication.rs private padded GHASH input and length encoding
│   ├── aead/gcm/limits.rs         private supported-input policy
│   ├── aead/gcm/tag.rs            private full-tag masking and comparison
│   ├── aead/gcm/seal.rs           private Algorithm 4 composition
│   ├── aead/gcm/open.rs           private verify-before-decrypt Algorithm 5 composition
│   ├── aead/gcm/ghash/field.rs    private SP 800-38D `GF(2^128)` multiplication
│   ├── aead/gcm/ghash/state.rs    private complete-block Algorithm 2 recurrence
│   ├── curve.rs                   shared elliptic-curve group boundary
│   ├── curve/p256/arithmetic.rs   private 256-bit limbs and fold-based modular reduction
│   ├── curve/p256/field.rs        private residues modulo `p` and canonical encoding
│   ├── curve/p256/scalar.rs       private residues modulo `n` and range rules
│   ├── curve/p256/point.rs        private complete addition, fixed multiplication, SEC 1 encoding
│   ├── agreement.rs               generic key-agreement contract and family boundary
│   ├── agreement/ecdh_p256/api.rs public typed ECDH P-256 keys, generation, and agreement
│   ├── agreement/x25519/api.rs    public typed X25519 agreement boundary
│   ├── agreement/x25519/field.rs  private `GF(2^255 - 19)` encoding and arithmetic
│   ├── agreement/x25519/scalar.rs private RFC 7748 scalar preparation
│   ├── agreement/x25519/ladder.rs private fixed-structure Montgomery ladder
│   ├── rsa/mod.rs                 RFC 8017 component owners and primitive boundary
│   ├── rsa/integer.rs             private base-2^32 integers and Montgomery `modpow`
│   ├── rsa/key.rs                 `RsaPublicKey`/`RsaPrivateKey` and RSAEP/RSADP/RSASP1/RSAVP1
│   ├── signature.rs               generic signing and verification contracts
│   ├── signature/rsa_pss/api.rs   typed PSS verifying key, signature, §8.1.2, modulus floor
│   ├── signature/rsa_pss/emsa.rs  §9.1.2 EMSA-PSS-VERIFY in numbered steps
│   ├── signature/rsa_pss/mgf1.rs  Appendix B.2.1 MGF1-SHA-256
│   ├── signature/ecdsa_p256/api.rs typed signing/verifying keys and raw `r || s` signature
│   ├── signature/ecdsa_p256/nonce.rs RFC 6979 §3.2 deterministic `k` generator
│   ├── signature/ecdsa_p256/sign.rs FIPS 186-5 §6.4.1 signing equation and retry loop
│   ├── signature/ecdsa_p256/verify.rs FIPS 186-5 §6.4.2 step sequence
│   ├── signature/ed25519/api.rs   typed keys, signing, and strict verification
│   ├── signature/ed25519/field.rs Edwards field encoding and root recovery
│   ├── signature/ed25519/point.rs point arithmetic and fixed scalar multiplication
│   ├── signature/ed25519/scalar.rs canonical and reduced subgroup scalars
│   ├── security.rs                lifecycle taxonomy, separate from audit status
│   └── ...
├── tests/
│   ├── sha256.rs                  public SHA-256 integration-test harness
│   ├── sha256/                    known-answer, boundary, streaming, and differential tests
│   ├── vectors/sha256/            published-vector provenance notes
│   ├── hmac_sha256.rs             public HMAC-SHA-256 integration-test harness
│   ├── hmac_sha256/               RFC vectors, streaming, verification, and differential tests
│   ├── vectors/hmac-sha256/       RFC 4231 vector provenance
│   ├── hkdf_sha256.rs             public HKDF-SHA-256 integration-test harness
│   ├── hkdf_sha256/               RFC vectors, bounds, composition, and differential tests
│   ├── vectors/hkdf-sha256/       RFC 5869 vector provenance
│   ├── aes128.rs                  public AES-128 integration-test harness
│   ├── aes128/                    known-answer, round-trip, and differential tests
│   ├── vectors/aes-128/           FIPS 197 and NIST AES vector provenance
│   ├── aes128_gcm.rs              public AES-128-GCM integration-test harness
│   ├── aes128_gcm/                known-answer, failure, and differential tests
│   ├── x25519.rs                  public X25519 integration-test harness
│   ├── x25519/                    known-answer, boundary, and differential tests
│   ├── vectors/ghash/             SP 800-38D GHASH evidence provenance
│   ├── vectors/gcm/               SP 800-38D GCM composition evidence provenance
│   ├── vectors/x25519/            RFC 7748 vector and errata provenance
│   ├── sha384.rs                  NIST, CAVP boundary, and differential SHA-384 evidence
│   ├── vectors/sha384/            FIPS 180-4 SHA-384 and CAVP provenance
│   ├── hmac_sha384.rs, hmac_sha384/ RFC 4231 SHA-384 cases, streaming, verification, differential
│   ├── vectors/hmac-sha384/       RFC 4231 provenance
│   ├── hkdf_sha384.rs, hkdf_sha384/ Wycheproof HKDF-SHA-384 cases, bounds, differential
│   ├── vectors/hkdf-sha384/       Wycheproof provenance
│   ├── sha512.rs                  published and differential SHA-512 evidence
│   ├── vectors/sha512/            FIPS 180-4 SHA-512 provenance
│   ├── ed25519.rs                 public Ed25519 evidence harness
│   ├── ed25519/                   RFC vectors, strict boundaries, and differential evidence
│   ├── vectors/ed25519/           RFC 8032 vector and errata provenance
│   ├── ecdh_p256.rs               public ECDH P-256 evidence harness
│   ├── ecdh_p256/                 RFC 5903, CAVP CDH/PKV fixtures, boundaries, differential
│   ├── vectors/ecdh-p256/         SP 800-56A, RFC 5903, and CAVP provenance
│   ├── ecdsa_p256.rs              public ECDSA P-256 evidence harness
│   ├── ecdsa_p256/                RFC 6979, CAVP SigGen/SigVer fixtures, boundaries, differential
│   ├── vectors/ecdsa-p256/        FIPS 186-5, RFC 6979, and CAVP provenance
│   ├── chacha20.rs                public ChaCha20 harness (Appendix A.1–A.2)
│   ├── poly1305.rs                public Poly1305 harness (Appendix A.3)
│   ├── chacha20poly1305.rs        public AEAD harness (§2.8.2, A.4–A.5, Wycheproof, differential)
│   ├── vectors/chacha20-poly1305/ RFC 8439 and Wycheproof provenance
│   ├── rsa_pss.rs                 public RSASSA-PSS evidence harness
│   ├── rsa_pss/                   CAVP SigVer/SigGen and Wycheproof fixtures and verdict tests
│   └── vectors/rsa-pss/           RFC 8017, CAVP, and Wycheproof provenance
├── DESIGN.md                      crate-wide architectural decisions
├── ROADMAP.md                     primitive implementation order
└── STANDARDS.md                   sources, notation, coverage, and evidence ledger
```

## Test placement

- Formula, schedule, round-state, and padding tests live beside their private implementation.
  They are white-box tests whose purpose is to make each layer inspectable.
- Public tests live in an algorithm-named directory and are compiled by the corresponding
  top-level integration-test harness. SHA-256, HMAC-SHA-256, HKDF-SHA-256, and AES-128 follow
  this layout.
- Published vector material and provenance live under `tests/vectors/<algorithm>/`. A vector
  record must name its source document, revision, section or case identifier, retrieval location,
  and any mechanical conversion applied to it.
- `STANDARDS.md` is the authoritative map from specification requirements to implementation
  items and evidence. It must distinguish implemented, partial, and planned coverage.
- Differential tests use a well-established implementation only from development dependencies.
  The reference implementation never calls that dependency.

## Rustdoc teaching contract

The crate landing page establishes the security status, learning order, byte-oriented boundary,
and first runnable examples. Each implemented algorithm landing page then follows a consistent
teaching sequence:

1. the problem and non-goals;
2. inputs, outputs, and security properties;
3. a mapping from standard notation to Rust names;
4. the algorithm steps in publication order;
5. a worked published example;
6. runnable public-API examples;
7. common mistakes and intentionally unsupported profiles;
8. exact standards and validation evidence; and
9. links into the private source layers that own each step.

Public traits and value types either contain a runnable example, link to their module's generic or
concrete example, or state honestly that no implementation exists yet. `cargo test -p rsl-crypto
--doc` is therefore part of the test contract. Strict documentation builds include private items
so broken source-map links fail validation instead of quietly degrading the rendered reference.

## Dependency direction

```text
constants ────────────────────────┬─> compression ─> state ─> public Sha256 API
                                 └──────────────────> state (initial words)
functions ─> schedule ───────────────> compression
          └──────────────────────────> compression

Sha256 ─> HMAC key normalization ─> inner/outer seeded states ─> public HmacSha256 API

HmacSha256 ─> HKDF Extract ─> secret PRK ─> bounded HKDF Expand

AES block bytes ─> four-by-four State ─> round transforms ─> AES-128 block API

GF(2^8) byte arithmetic ────────────────> round transforms and calculated S-box

GF(2^128) block multiplication ─> GHASH recurrence ─> AES-128-GCM authentication

GCM CounterBlock ─> inc32 ─> GCTR keystream ───────> AES-128-GCM confidentiality

AAD/ciphertext padding + lengths ─> GHASH ─> tag mask/verify ─> public AES-128-GCM

GF(2^255 - 19) bytes ─> field arithmetic ─> scalar-prepared Montgomery ladder
                                              └─> checked public X25519 agreement

SHA-512 ─> seed expansion + nonce/challenge hashes ─┐
                                                    ├─> Ed25519 signing/strict verification
Edwards field ─> point encoding/addition ─> scalar multiplication
Subgroup order L ─> canonical/reduced scalar arithmetic ────────────────┘
```

Dependencies point toward composition. A lower layer must not depend on streaming state or the
public API. This keeps every transformation testable without constructing an entire hash.

## Export rule

The SHA-256, SHA-512, HMAC-SHA-256, HKDF-SHA-256, AES-128, AES-128-GCM, X25519, and Ed25519
modules export their
algorithm and distinct input/output types where semantic separation matters. They became public
only after each readable layer was implemented and its applicable known-answer,
boundary/negative, fragmentation, round-trip or agreement, and differential tests passed. Future
algorithms follow the same evidence-before-export rule. Public export records implementation
status; it does not claim production readiness.

## Domain boundary

The SHA-256 *compression function* is an internal cryptographic operation, so `compression.rs`
lives here in `rsl-crypto`. It is unrelated to reversible data-compression formats and must not
move into the top-level `rsl-compression` crate. TLS and SSH transcript hashing call the public
digest API from their protocol crates; protocol-specific state does not move here.
