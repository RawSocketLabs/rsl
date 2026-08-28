# rsl-crypto — roadmap

The first goal is an inspectable AES-128-GCM protection path suitable for later TLS and SSH
record/packet contexts. Every layer lands independently and remains testable on its own.

## Validation convention

Each algorithm change must include:

- published known-answer vectors with source and vector identifiers recorded in the test;
- boundary and malformed-input tests;
- intermediate-state assertions wherever the source standard publishes intermediate values;
- differential tests against at least one established implementation before protocol use;
- no claim of side-channel resistance or production suitability without a separate review.

## Initial dependency chain

1. **SHA-256** — direct FIPS 180-4 implementation, proceeding through constants and elementary
   word functions, message scheduling, compression, and incremental state in that order. The
   eight initial hash words, sixty-four round constants, and six elementary word functions are
   implemented and tested. Complete blocks are parsed into schedule words 0 through 15, and the
   full expansion through word 63 is implemented and tested. Compression working variables, one
   explicit round, the 64-round loop, feed-forward, and complete-block compression are implemented
   and tested. Preprocessing, checked incremental state, padding, digest serialization, and the
   public `Sha256`/`Sha256Digest` types are implemented. Public validation includes NIST example
   digests, NIST CAVP boundary vectors, fragmentation tests, and a development-only differential
   comparison. The initial readable SHA-256 vertical slice is complete; broader production
   assurance remains deliberately out of scope for that milestone.
2. **HMAC-SHA-256** — the FIPS 198-1/RFC 2104 construction is implemented as visible secret key
   normalization, explicit inner/outer pad XOR, and separately seeded SHA-256 states. It exports
   a distinct full-tag type, one-shot and incremental authentication, and full-tag verification.
   All seven RFC 4231 cases are covered with the truncated case labeled as prefix-only; public
   fragmentation, negative verification, and RustCrypto differential tests pass. The initial
   readable HMAC-SHA-256 vertical slice is complete, with compiler-level constant-time analysis
   and independent audit still required for a production claim.
3. **HKDF-SHA-256** — visibly separate RFC 5869 Extract and Expand operations are implemented,
   along with a convenience composition that preserves those public boundaries. PRKs and
   recurrence blocks use zeroizing secret storage; output above 8,160 bytes is rejected before
   mutation. All three SHA-256 Appendix A PRK/OKM pairs, exact-limit boundaries, stage
   composition, and RustCrypto differential tests pass. The initial readable slice is complete.
4. **AES-128** — the direct FIPS 197-upd1 state mapping, calculated field arithmetic and S-boxes,
   all forward and inverse transforms, 44-word key expansion, `CIPHER()`, and `INVCIPHER()` are
   implemented as separate readable layers. Production substitution uses calculation rather than
   secret-indexed lookup tables. Evidence covers all published S-box entries, every Appendix A.1
   expanded word, Appendix B round boundaries and final block, supplementary NIST forward and
   inverse examples, exhaustive transform cancellation properties, and varied complete-cipher
   round trips. The public raw-block API uses distinct, owned, non-`Clone`, zeroizing key and block
   types and has known-answer and development-only RustCrypto differential tests in both
   directions. The initial readable AES-128 slice is complete; the API explicitly supplies no
   mode, nonce, authentication, padding, framing, or arbitrary-length message encryption.
5. **GHASH** — the exact 128-iteration block multiplication from SP 800-38D §6.3 Algorithm 1 is
   implemented as a private, byte-oriented, zeroizing `GF(2^128)` layer. Its tests make the
   standard's counterintuitive displayed-bit/polynomial mapping explicit, exercise both reduction
   branches, verify zero and the correctly encoded field identity, and apply Algorithm 1 to
   operands published in NIST's GCM-AES128 Example 2. That individual expected product is labeled
   standard-derived because NIST publishes the operands but not the intermediate result. A
   distinct secret hash-subkey owner and accumulator now implement §6.4 Algorithm 2 one complete
   block at a time, reaching Example 2's NIST-published final `S` value over its four ciphertext
   blocks and length block. A development-only differential test agrees with RustCrypto `ghash`
   0.6.0 over 32 deterministic cases containing one through eight blocks. The initial
   complete-block GHASH slice is complete; it remains private and cannot be used as a standalone
   hash. GCM input padding and length-block construction belong to the later composition layer.
6. **AES-128-GCM** — the initial 96-bit-nonce, 128-bit-tag profile is complete. Private layers keep
   §6.2 `inc32`, byte-aligned §6.5 GCTR, Algorithms 4/5 key-and-IV setup, independent AAD and
   ciphertext padding, bit-length encoding, §5.2.1 input limits, tag masking, and full-tag
   comparison visible and independently tested. Private `seal` composes Algorithm 4 in printed
   order. Private `open` uses Algorithm 5's explicitly permitted verify-before-plaintext order, so
   authentication failure creates no plaintext owner. Public `Aes128GcmKey`, `Aes128GcmNonce`,
   `Aes128GcmTag`, and `Aes128Gcm` types expose detached authenticated encryption through both
   inherent methods and the crate-wide `Aead` contract. Published NIST Examples 1–5 cover every
   intermediate `S` and full tag; public Examples 1 and 5 exercise complete construction. Negative
   tests alter AAD, nonce, every ciphertext byte, and every tag byte. A development-only
   differential test agrees with `RustCrypto` `aes-gcm` 0.11.1 over 32 varied keys, nonces, AAD
   lengths, and payload lengths. Variable-length IVs and truncated tags remain deliberately
   unsupported. Protocol-specific nonce construction, uniqueness, counter exhaustion, replay
   handling, record/packet framing, and activation rules remain in the TLS or SSH repository.
7. **X25519** — the RFC 7748 field representation, coordinate decoding, scalar preparation,
   fixed-structure Montgomery ladder, affine recovery, typed public boundary, and all-zero shared
   result rejection are implemented as separate readable layers. Public evidence covers both
   direct §5.2 vectors, its one- and 1,000-iteration checkpoints, the complete §6.1 Alice/Bob
   exchange, non-canonical and high-bit input behavior, exact wire length, rejection, generic
   dispatch, and development-only differential comparison with `x25519-dalek` 3.0.0. Private and
   shared-secret owners are non-`Clone`, redacted, and zeroizing. Protocol repositories still own
   entropy adapters, ephemeral-key lifetime, transcript authentication, key-share framing, and
   KDF inputs.
8. **SHA-512 and Ed25519** — SHA-512 is implemented as a complete independent FIPS 180-4 path with
   64-bit functions, eighty constants and rounds, 128-byte blocks, 128-bit length accounting,
   published examples, fragmentation evidence, and differential testing. Pure Ed25519 implements
   RFC 8032 canonical field and point decoding, complete extended-coordinate addition,
   fixed-structure scalar multiplication, scalar reduction modulo `L`, private-seed expansion,
   deterministic signing, strict verification, typed wire values, caller-provided seed entropy,
   RFC §7.1 vectors, malformed-input tests, and differential comparison with `ed25519-dalek`
   3.0.0. Ed25519ctx and Ed25519ph are exposed as distinct methods over the same signing core
   with a validated `Ed25519Context`, covered by the RFC §7.2 and §7.3 vectors and a differential
   prehashed suite. TLS/SSH transcript construction, key identifiers, certificates, and
   negotiation remain protocol-owned.

9. **P-256, ECDH, and ECDSA verification** — the SP 800-186 curve is implemented once as
   readable 256-bit limb arithmetic with a fold-based reduction derived from the prime's form,
   distinct field and scalar residue types, Renes–Costello–Batina complete projective addition
   in its printed 43-step order, and fixed-structure scalar multiplication. ECDH exposes
   `EcdhP256PrivateKey`, `EcdhP256PublicKey`, `EcdhP256SharedSecret`, and `EcdhP256` with
   SP 800-56A Rev. 3 candidate-testing generation, full public-key validation, and the ECC CDH
   primitive. ECDSA exposes `EcdsaP256SigningKey`, `EcdsaP256VerifyingKey`, and `EcdsaP256Signature`
   with FIPS 186-5 §6.4.1 signing under RFC 6979 deterministic `k`, §6.4.2 verification over
   SHA-256, and a raw `r || s` encoding. Evidence covers RFC 5903 §8.1, all 25 CAVP ECC CDH and
   12 CAVP PKV P-256 cases, RFC 6979 A.2.5 (published `k` values and exact signatures), all 15
   CAVP SigGen `(d, k) -> (r, s)` cases and 15 CAVP SigVer verdicts, range and tampering
   boundaries, and development-only byte-identical differential comparison with the `p256`
   crate 0.14.0. Randomized `k`, DER signature framing, compressed points, and other curves
   and hashes remain deliberately unsupported.

10. **RSA primitive and RSASSA-PSS verification** — the RFC 8017 integer engine (`BigUint`,
    Montgomery `modpow`) and `RsaPublicKey`/`RsaPrivateKey` owners moved from `rsl-crypto-legacy`
    into `rsl-crypto::rsa` so one exponentiation serves both contemporary PSS verification and
    the opt-in historical PKCS #1 v1.5 encodings, which now attach through extension traits.
    `signature::rsa_pss` exposes `RsaPssSha256VerifyingKey` and `RsaPssSignature` with §8.1.2
    verification, §9.1.2 EMSA-PSS-VERIFY in numbered steps, Appendix B.2.1 MGF1, a 2048-bit
    modulus floor, and a default `sLen = hLen` with explicit-salt-length entry points. Evidence
    covers all 18 CAVP `SigVerPSS` 2048/SHA-256 verdicts, all 10 CAVP `SigGenPSS` 2048/SHA-256
    signatures with their 20-byte salts, and all 108 Wycheproof `rsa_pss_2048_sha256_mgf1_32`
    cases. Signing, other hashes/MGFs, and `RSASSA-PSS-params` ASN.1 remain unsupported.

11. **ChaCha20, Poly1305, and AEAD_CHACHA20_POLY1305** — RFC 8439 in three public layers:
    `cipher::chacha20` (quarter round, block function, one-shot and stateful keystream with a
    hard counter-wrap refusal), `mac::poly1305` (clamping, 44-bit-limb accumulation folding
    `2^130 ≡ 5`, uniform verification), and `aead::chacha20poly1305` (§2.6 key derivation and
    the §2.8 composition behind the shared `Aead` contract, tag verified before decryption).
    Evidence covers every RFC body intermediate, all Appendix A vectors, all 325 Wycheproof
    cases, tampering boundaries, and differential comparison with `chacha20poly1305` 0.11.0.

12. **SHA-384, HMAC-SHA-384, HKDF-SHA-384** — SHA-384 reuses SHA-512's padding and compression
    with its own initial words and a truncated output (the one deliberate sharing in the SHA-2
    family, because FIPS 180-4 §6.5 defines it that way); HMAC and HKDF are parameter clones of
    the SHA-256 profiles. Evidence: NIST examples including the discarded words, CAVP boundary
    lengths, RFC 4231 cases 1–7, all 83 Wycheproof HKDF-SHA-384 cases, and differential tests.

13. **AES-256 and AES-256-GCM** — AES-256 adds only the `Nk = 8` key schedule (with Algorithm
    2's AES-256-only `SUBWORD` step) over the shared AES layers, whose `CIPHER()`/`INVCIPHER()`
    bodies are now generic over a round-key source. GCM's private layers became generic over a
    crate-private `CIPH_K` trait, so `Aes256Gcm` is a second typed profile of the same code.
    Evidence: all 60 Appendix A.3 words, the four `AES_Core256` blocks, NIST GCM-AES256
    Examples 1–5, 105 Wycheproof cases, and differential `aes`/`aes-gcm` comparison.

## Performance policy

Backends stay pure Rust with `#![forbid(unsafe_code)]`, no intrinsics, and no secret-indexed
tables. Speed only needs to be adequate for real TLS and SSH clients and servers; readability,
API clarity, and flexibility come first, so no optimized implementation is planned beside the
reference path. Simple algorithmic choices (limb arithmetic instead of bit-serial loops) are
acceptable when they keep the code and its tests clear and fast.

## Widening the TLS 1.3 / SSH algorithm set

The next slices add the remaining commonly negotiated algorithms, each with the same evidence
bar:

1. P-384 ECDH and ECDSA-SHA-384, reusing the P-256 module structure.
2. X448 and Ed448 as lower-priority interoperability profiles.

## After the first vertical slice

- The quarantined historical-cryptography sequence is tracked in `LEGACY-ROADMAP.md`.
