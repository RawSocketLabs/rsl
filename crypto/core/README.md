# crypto

An accuracy-first cryptographic library for the RSL stack. Its current consumer is
certificate-path validation in `pki`; TLS and SSH record/packet protection are the intended next
consumers, with framing and connection state staying in their protocol crates.

The goal is readable, independently testable reference code: specifications should map directly
to named types, operations, and intermediate values. Speed is secondary: backends stay pure Rust
and readable (see the [`ROADMAP.md`](ROADMAP.md) performance policy).

## Why this library exists

Cryptographic libraries make different primary tradeoffs:

| Library family | Primarily optimizes for |
|---|---|
| [RustCrypto](https://github.com/RustCrypto) | Broad, modular, pure-Rust algorithms and reusable traits |
| [libsodium](https://doc.libsodium.org/doc) | Portable, high-level operations designed for ease of use |
| [`ring`](https://github.com/briansmith/ring) / [AWS-LC for Rust](https://github.com/aws/aws-lc-rs) | Native optimized providers; AWS-LC for Rust additionally targets FIPS requirements |
| [EverCrypt](https://project-everest.github.io/) | Formally verified implementations with platform-selected performance |
| **`crypto`** | Traceable protocol engineering: readable standards mappings, semantic lifecycle APIs, and explicit research escape hatches |

`crypto` exists for work where developers need to follow a construction from publication to
source and test evidence, compose it into protocol machinery without losing security state, and
still reach lower-level operations for vectors, interoperability, and new protocol research. The
guided path makes complete constructions conspicuous; an escape hatch returns only the narrower
property it actually establishes and cannot fabricate authenticated or protocol-validated state.

The result is an executable reference and protocol workbench—not a present claim of independent
audit, formal verification, FIPS validation, or production-ready side-channel resistance.

## Status

Early implementation. The crate exports digests (SHA-256/384/512, SHA3-256, SHAKE256), MACs and
key derivation (HMAC and HKDF over SHA-256/384, Poly1305), ciphers and AEADs (AES-128/256,
ChaCha20, AES-GCM, ChaCha20-Poly1305, `aead::record`), key agreement (X25519, X448, ECDH
P-256/P-384), and signatures (Ed25519, Ed448, ECDSA P-256/P-384, RSASSA-PSS verification), plus
the shared RSA primitive behind the opt-in PKCS #1 v1.5 encodings in `crypto-legacy`.
[`ROADMAP.md`](ROADMAP.md) records each slice, [`STANDARDS.md`](STANDARDS.md) is the canonical
per-algorithm evidence ledger, and `tests/vectors/*/README.md` records vector provenance. See
[`GUIDE.md`](GUIDE.md) for task-to-algorithm selection and [`STRUCTURE.md`](STRUCTURE.md) for code
boundaries.

This is an implementation milestone, not a production-security claim. [`SIDE-CHANNELS.md`](SIDE-CHANNELS.md)
records a source-level review of every secret-dependent branch, and [`../fuzz/`](../fuzz/README.md)
holds libFuzzer targets for every untrusted-input boundary; measured timing analysis, broader
interoperability work, and independent cryptographic audit are still required before production
use.

## Choose an algorithm

[`GUIDE.md`](GUIDE.md) starts from the protocol task, compares the implemented families, records
real TLS/SSH pairings, and marks combinations that must never be assembled. The module pages remain
the source for mechanics; the guide is only the selection layer.

## Protect a large byte stream

[`Aead::seal`](src/aead.rs) already accepts an arbitrary-length in-memory byte slice; callers do
not split text into AES blocks. For bounded-memory input, [`aead::record`](src/aead/record.rs)
provides a staged builder, `write_to` over arbitrary fragments, one-at-a-time delivery to a
caller-selected fallible `RecordSink`, and a consuming `finish_to` transition. Sink failure
invalidates the stream so a nonce cannot be retried. On receipt, `open_data_to` authenticates each
record before moving plaintext into a fallible `RecordPlaintextSink`; `open_final_to` consumes the
opener. A plaintext-sink failure invalidates the opener so a record cannot be delivered twice. The
collecting methods remain conveniences. This layer defines no wire encoding. Protocol crates still
own negotiation, framing, key derivation and lifetime, transcript binding, replay, and rekeying.

## Guided APIs and escape hatches

Complete constructions are the short path, while raw ciphers, incremental hash/MAC/KDF stages,
detached AEAD parts, algorithm-specific key import, caller-selected contexts, record sizing, and
explicit secret exposure remain available for test vectors and new protocol research. These
lower-level APIs retain narrow result types: using an escape hatch never silently turns raw output
into an authenticated or protocol-validated value. [`DESIGN.md`](DESIGN.md) records the rule.

## Read the teaching reference

The crate-level rustdoc is the intended starting point. It gives a dependency-ordered learning
path, then every implemented algorithm page answers the same questions: what problem the
algorithm solves, what security it does and does not provide, how the publication's notation maps
to Rust, how the steps compose, what a published worked example produces, how to call the public
API, which mistakes to avoid, and exactly which evidence supports the implementation.

Build the complete reference, including the private specification layers linked by each source
map, with:

```console
cargo doc -p crypto --no-deps --document-private-items --open
```

All Rust examples in the crate rustdoc are compiled and run as doctests:

```console
cargo test -p crypto --doc
```

The docs.rs package metadata also enables private-item documentation so hosted source-map links
lead to the small internal operations being taught. Private visibility remains enforced by Rust;
documenting those items does not make them callable by downstream crates.

## Examples

Each module's rustdoc carries its tested examples (`cargo doc -p crypto --open`). The two
showcases below are not compiled from this file; their tested copies live in the crate and module rustdoc.

### AES-128-GCM

```rust,ignore
use crypto::{
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
for each encryption under a key and enforce record/packet counter exhaustion. This profile
intentionally does not expose variable-length IVs or truncated tags.

### Ed25519

```rust,ignore
use crypto::signature::ed25519::Ed25519SigningKey;

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
non-canonical scalars, and small-order points. TLS and SSH crates still own transcript
encoding, public-key identity, certificates/key blobs, and signature-algorithm negotiation.
