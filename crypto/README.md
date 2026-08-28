# rsl-crypto

An accuracy-first cryptographic library for the RSL stack. Its first consumers will be
record/packet protection in TLS and SSH, but protocol framing and connection state remain in
their protocol crates.

The goal is readable, independently testable reference code: specifications should map directly
to named types, operations, and intermediate values. Speed is secondary. Optimized backends may
be added later beside the reference implementations.

## Status

Early implementation. The crate establishes secret handling and contracts for digests, MACs,
key derivation, block and stream ciphers, authenticated encryption, and key agreement. It now
exports complete readable
[`Sha256`](src/digest/sha2/sha256/state.rs),
[`Sha512`](src/digest/sha2/sha512/state.rs), and
[`HmacSha256`](src/mac/hmac/sha256/state.rs) reference paths with one-shot and incremental input,
checked length accounting, and distinct digest/tag output types. It also exports visibly separate
HKDF-SHA-256 Extract and Expand stages with a zeroizing PRK type and bounded caller output. The
first raw block-cipher API now exposes distinct [`Aes128Key`](src/cipher/aes/aes128/api.rs),
[`Aes128Block`](src/cipher/aes/aes128/api.rs), and
[`Aes128`](src/cipher/aes/aes128/api.rs) types over the fully inspectable FIPS 197-upd1
transformation layers. The first authenticated-encryption API exposes distinct
[`Aes128GcmKey`](src/aead/gcm/api.rs), [`Aes128GcmNonce`](src/aead/gcm/api.rs),
[`Aes128GcmTag`](src/aead/gcm/api.rs), and [`Aes128Gcm`](src/aead/gcm/api.rs) types for the
SP 800-38D profile with a 96-bit nonce and full 128-bit tag. The first key-agreement API exposes
distinct [`X25519PrivateKey`](src/agreement/x25519/api.rs),
[`X25519PublicKey`](src/agreement/x25519/api.rs),
[`X25519SharedSecret`](src/agreement/x25519/api.rs), and
[`X25519`](src/agreement/x25519/api.rs) types over a fully inspectable RFC 7748 field and
Montgomery-ladder path. The first signature implementation exposes
[`Ed25519SigningKey`](src/signature/ed25519/api.rs),
[`Ed25519VerifyingKey`](src/signature/ed25519/api.rs), and
[`Ed25519Signature`](src/signature/ed25519/api.rs) over readable RFC 8032 Edwards and subgroup
scalar layers. The NIST P-256 group is implemented once under [`src/curve/p256/`](src/curve/p256/)
and consumed by [`EcdhP256`](src/agreement/ecdh_p256/api.rs) (SP 800-56A ECC CDH with full
public-key validation) and [`EcdsaP256SigningKey`](src/signature/ecdsa_p256/api.rs) /
[`EcdsaP256VerifyingKey`](src/signature/ecdsa_p256/api.rs) (FIPS 186-5 signing with RFC 6979
deterministic `k` and verification, both with SHA-256).

SHA-256 is tested at several levels: private formula and intermediate-state tests,
NIST-published known answers, NIST CAVP padding/block-boundary vectors, awkward input
fragmentation, and differential comparison with a development-only established implementation.
See [`STRUCTURE.md`](STRUCTURE.md) for code boundaries, [`STANDARDS.md`](STANDARDS.md) for exact
section coverage, and [`tests/vectors/sha256/README.md`](tests/vectors/sha256/README.md) for vector
provenance. HMAC-SHA-256 adds focused secret-key normalization/pad tests, every RFC 4231 case,
full-tag verification failures, message fragmentation, and an independent differential oracle.
AES-128 adds every Appendix A.1 expanded-key word, Appendix B and supplementary NIST published
block examples, inspectable round boundaries, forward/inverse cancellation, and a development-only
differential oracle. AES-128-GCM adds published end-to-end encryption/decryption cases, explicit
authentication-failure evidence for every changed tag and ciphertext byte, and 32 varied
development-only differential cases. X25519 adds both direct RFC vectors, iterative checkpoints,
the published two-party exchange, encoding/rejection boundaries, and 128 development-only
differential cases. SHA-512 adds published one- and two-block examples, 128-bit padding boundaries,
fragmentation, and independent differential cases. Ed25519 adds RFC 8032 §7.1 key/signature
vectors, canonical and small-order rejection evidence, deterministic and generic signing paths,
and 32 strict differential cases. ECDH P-256 adds the RFC 5903 exchange, all 25 CAVP ECC CDH and
12 CAVP PKV P-256 cases, range and validation boundaries, and 32 differential cases. ECDSA P-256
adds RFC 6979 A.2.5 `k` values and exact signatures, all 15 CAVP SigGen `(d, k) -> (r, s)` cases,
all 15 CAVP SigVer verdicts, range and tampering boundaries, and 32 byte-identical differential
signatures.

This is an implementation milestone, not a production-security claim. Side-channel analysis,
fuzzing, broader interoperability work, and independent cryptographic audit are still required
before production use.

## Read the teaching reference

The crate-level rustdoc is the intended starting point. It gives a dependency-ordered learning
path, then every implemented algorithm page answers the same questions: what problem the
algorithm solves, what security it does and does not provide, how the publication's notation maps
to Rust, how the steps compose, what a published worked example produces, how to call the public
API, which mistakes to avoid, and exactly which evidence supports the implementation.

Build the complete reference, including the private specification layers linked by each source
map, with:

```console
cargo doc -p rsl-crypto --no-deps --document-private-items --open
```

All displayed Rust examples are compiled and run as doctests:

```console
cargo test -p rsl-crypto --doc
```

The docs.rs package metadata also enables private-item documentation so hosted source-map links
lead to the small internal operations being taught. Private visibility remains enforced by Rust;
documenting those items does not make them callable by downstream crates.

Initial vertical slice:

1. SHA-256
2. HMAC-SHA-256
3. HKDF-SHA-256
4. AES-128
5. GHASH
6. AES-128-GCM
7. X25519
8. SHA-512
9. Ed25519

All nine items are complete at the initial evidence level. The AES-128-GCM path retains private,
independently testable layers for `GF(2^128)` multiplication, GHASH recurrence, `inc32`, GCTR,
96-bit-IV setup, padded AAD/ciphertext authentication input, input limits, tag masking and
comparison, authenticated encryption, and verify-before-decrypt. Those layers connect every
published `S`/tag pair in NIST's five full-tag examples and expose only the complete construction;
GHASH cannot be used as a standalone public hash. Each implementation and test identifies whether
its evidence is published by the controlling standard, derived from a published rule, a local
regression case, or a differential comparison.

## Current SHA-256 API

```rust
use rsl_crypto::digest::sha2::sha256::Sha256;

let one_shot = Sha256::digest("hello").expect("a short message fits SHA-256");

let mut incremental = Sha256::new();
incremental.update("hel").expect("a short message fits");
incremental.update(String::from("lo")).expect("a short message fits");

assert_eq!(incremental.finalize(), one_shot);
assert_eq!(one_shot.as_bytes().len(), 32);
```

Input is borrowed as its existing byte representation via `AsRef<[u8]>`; text is therefore
hashed as UTF-8 bytes. The API does not serialize arbitrary Rust values because that would make
their byte representation ambiguous. Wire structures should first be encoded by `bitsandbytes`,
then the resulting bytes can be hashed.

## Current SHA-512 API

```rust
use rsl_crypto::digest::sha2::sha512::Sha512;

let one_shot = Sha512::digest("hello").expect("a short message fits SHA-512");
let mut incremental = Sha512::new();
incremental.update("hel").expect("a short message fits");
incremental.update("lo").expect("a short message fits");
assert_eq!(incremental.finalize(), one_shot);
assert_eq!(one_shot.as_bytes().len(), 64);
```

SHA-512 is a complete public digest rather than an opaque Ed25519 dependency. Its separate
64-bit functions, eighty-round schedule, 128-byte blocks, 128-bit length field, and 64-byte typed
output remain visible in the teaching source.

## Current HMAC-SHA-256 API

```rust
use rsl_crypto::{
    SecretBytes,
    mac::hmac::sha256::HmacSha256,
};

let key = SecretBytes::new(*b"shared secret");
let tag = HmacSha256::authenticate(key.expose_secret(), "message")
    .expect("a short message fits HMAC-SHA-256");

let mut verifier = HmacSha256::new(key.expose_secret()).expect("the key fits SHA-256");
verifier.update("mes").expect("a short message fits");
verifier.update("sage").expect("a short message fits");
verifier.verify(tag.as_bytes()).expect("the full tag matches");
```

Key bytes are borrowed explicitly; the caller remains responsible for their source storage. The
HMAC state copies normalized key material into redacted, zeroizing storage and never retains the
borrow. Message input accepts `AsRef<[u8]>` for the same unambiguous byte-oriented behavior as
SHA-256. The first API exposes full 256-bit tags only; truncation policy belongs to a construction
or protocol that specifies it.

## Current HKDF-SHA-256 API

```rust
use rsl_crypto::kdf::hkdf::sha256::{derive, extract};

let prk = extract(Some(b"public salt"), b"secret input keying material")
    .expect("the input fits HMAC-SHA-256");
let mut explicit_output = [0_u8; 42];
prk.expand(b"protocol context", &mut explicit_output)
    .expect("42 bytes are within HKDF-SHA-256's limit");

let mut convenient_output = [0_u8; 42];
derive(
    Some(b"public salt"),
    b"secret input keying material",
    b"protocol context",
    &mut convenient_output,
)
.expect("the inputs and output length are valid");

assert_eq!(convenient_output, explicit_output);
```

Extract and Expand remain separately named because the PRK boundary matters in real protocol key
schedules. The PRK and recurrence blocks use redacted, zeroizing storage. Caller-owned input and
output buffers remain the caller's destruction responsibility. Output is capped at the RFC 5869
limit of 8,160 bytes, and protocol-specific `info` encoding stays in the protocol repository.

## Current X25519 API

```rust
use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey};

// Reproducible example inputs only. A protocol integration must generate ephemeral private bytes
// with an approved cryptographic random source.
let alice_private = X25519PrivateKey::new([0x11; 32]);
let bob_private = X25519PrivateKey::new([0x22; 32]);
let alice_public = X25519::public_key(&alice_private);
let bob_public = X25519::public_key(&bob_private);

let alice_shared = X25519::agree(&alice_private, &bob_public)
    .expect("the peer coordinate produces a nonzero shared secret");
let bob_shared = X25519::agree(&bob_private, &alice_public)
    .expect("the peer coordinate produces a nonzero shared secret");
assert_eq!(alice_shared.expose_secret(), bob_shared.expose_secret());
```

Private keys and shared secrets are redacted, non-`Clone`, and zeroized on drop. Public coordinate
parsing enforces exactly 32 bytes but intentionally accepts non-canonical encodings as RFC 7748
requires. Agreement rejects an all-zero result. The protocol repository remains responsible for
random key generation, ephemeral-key reuse rules, key-share framing, transcript authentication,
and feeding the secret plus required public context into its specified KDF.

## Current Ed25519 API

```rust
use rsl_crypto::signature::ed25519::Ed25519SigningKey;

// Reproducible example only. Real seeds come from an approved RandomSource adapter.
let signing_key = Ed25519SigningKey::from_seed([0x42; 32]);
let verifying_key = signing_key.verifying_key();
let signature = signing_key
    .sign(b"exact transcript bytes")
    .expect("a short message fits SHA-512");
verifying_key
    .verify(b"exact transcript bytes", &signature)
    .expect("the signature and message match");
```

Signing is deterministic pure Ed25519; it does not consume per-signature randomness and does not
silently switch to Ed25519ctx or Ed25519ph. Verification rejects malformed encodings,
non-canonical scalars, and small-order points. TLS and SSH repositories still own transcript
encoding, public-key identity, certificates/key blobs, and signature-algorithm negotiation.

## Current AES-128 API

```rust
use rsl_crypto::cipher::aes::aes128::{Aes128, Aes128Block, Aes128Key};

let cipher = Aes128::new(Aes128Key::new(*b"0123456789abcdef"));
let mut block = Aes128Block::new(*b"one-block-input!");

cipher.encrypt_block(&mut block);
assert_ne!(block.as_bytes(), b"one-block-input!");

cipher.decrypt_block(&mut block);
assert_eq!(block.into_bytes(), *b"one-block-input!");
```

The separate key and block types prevent accidental interchange even though both contain sixteen
bytes. Keys and blocks are owned, non-`Clone`, and zeroized on drop; taking block bytes out makes
the caller responsible for their remaining lifetime. This API is only the FIPS 197 block
permutation: it provides no nonce, mode, integrity, padding, framing, or arbitrary-length message
encryption. Applications should use an authenticated construction such as the AES-128-GCM layer
below rather than apply raw AES directly.

## Current AES-128-GCM API

```rust
use rsl_crypto::{
    RandomSource, Result,
    aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce},
};

fn protect_once(random: &mut impl RandomSource) -> Result<()> {
    let algorithm = Aes128Gcm::new(Aes128GcmKey::generate(random)?);
    let nonce = Aes128GcmNonce::generate(random)?;

    // A protocol may write this header unchanged while binding its exact bytes as AAD.
    let cleartext_header = b"visible header";
    let sealed = algorithm.seal(&nonce, cleartext_header, b"protected payload")?;

    // A detached wire layout can be: cleartext_header || nonce || ciphertext || tag.
    assert_eq!(sealed.tag().as_bytes().len(), 16);

    let plaintext = algorithm.open(
        &nonce,
        cleartext_header,
        sealed.ciphertext(),
        sealed.tag(),
    )?;
    assert_eq!(plaintext, b"protected payload");
    Ok(())
}
```

The key, nonce, and tag are different exact-size types. Associated data is never encrypted, but
changing it causes authentication failure. `open` authenticates before allocating or transforming
plaintext. A decoder can use `Aes128GcmNonce::try_from(nonce_slice)` and
`Aes128GcmTag::try_from(tag_slice)` to turn exact wire slices into typed values; wrong lengths are
rejected before cryptographic work. `Aes128GcmKey::new` and `Aes128GcmNonce::new` remain available
for exact published vectors and protocol-derived values. Random nonce generation is a convenience,
not reuse tracking: the consuming TLS, SSH, or other protocol context must construct a fresh nonce
for each encryption under a key and enforce record/packet counter exhaustion. This first profile
intentionally does not expose variable-length IVs or truncated tags.
