# rsl-crypto — design

## 1. Purpose

Provide understandable, exact cryptographic primitives that protocol crates can compose into
TLS, SSH, and other protection schemes. This is an executable-specification project first and a
performance project second.

## 2. Boundaries

This crate owns primitives and secret-bearing value types. A protocol crate owns negotiation,
transcript/session state, nonce construction, record or packet framing, sequence advancement,
and key activation. AES-GCM may be shared by TLS and SSH; their use of AES-GCM may not.

`bitsandbytes` remains the wire-format layer. Typed protocol data is encoded to bytes before it
enters a protection context and decoded only after authenticated opening succeeds.

SHA-256's compression function is an internal step of a cryptographic digest and belongs in this
crate. Despite the shared word “compression,” it is not a data-compression algorithm and has no
dependency on the separate `rsl-compression` crate.

## 3. Reference implementation policy

- Prefer pseudocode-shaped functions and named intermediate values.
- Avoid lookup tables indexed by secrets, platform intrinsics, assembly, and fused operations in
  the reference implementation.
- Keep algorithm state private but expose test-only or deliberately public diagnostic snapshots
  when published vectors require intermediate validation.
- Validate each layer independently before composing it into the next layer.

## 4. Security posture

Readable and vector-correct does not imply side-channel resistant or production-safe. Production
claims require constant-time analysis, secret-lifetime review, fuzzing, differential testing,
interoperability testing, and independent audit. Documentation must keep that distinction clear.

## 5. Initial dependency direction

```text
SHA-256
  -> HMAC-SHA-256
      -> HKDF-SHA-256

AES-128
  + GHASH
      -> AES-128-GCM

GF(2^255 - 19) arithmetic
  -> X25519 Montgomery ladder
      -> checked shared secret

SHA-512
  + GF(2^255 - 19) arithmetic
      -> Edwards25519 points
          -> Ed25519 key derivation, signing, and strict verification
```

Higher-level protocol key schedules consume these primitives without becoming dependencies of
this crate.

## 6. Algorithm lifecycle separation

`SecurityStatus` distinguishes recommended, legacy, broken, and educational-only algorithms.
The status describes lifecycle, not audit readiness. Historical primitives live in the separate
`rsl-crypto-legacy` package, are never included in a default protocol allowlist, and cannot be
selected by an implicit fallback. Protocol crates own the explicit act of enabling a historical
cipher suite; primitive crates only perform the named transformation.
