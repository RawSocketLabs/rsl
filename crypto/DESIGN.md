# rsl-crypto — design

## 1. Purpose

Provide understandable, exact cryptographic primitives that protocol crates can compose into
TLS, SSH, and other protection schemes. This is an executable-specification project first and a
performance project second.

## 2. Boundaries

This crate owns primitives, secret-bearing value types, and narrowly defined wire-independent
contracts such as generic AEAD record streaming. A protocol crate owns negotiation,
transcript/session state, its selected nonce and AAD profile, record or packet framing, replay,
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

ChaCha20 quarter round -> block function -> keystream
  + Poly1305 one-time authenticator
      -> AEAD_CHACHA20_POLY1305

GF(2^255 - 19) arithmetic
  -> X25519 Montgomery ladder
      -> checked shared secret

SHA-512
  + GF(2^255 - 19) arithmetic
      -> Edwards25519 points
          -> Ed25519 key derivation, signing, and strict verification

256-bit limb arithmetic modulo p and n
  -> P-256 field, scalar, and complete projective points
      -> ECDH P-256 validated agreement
      -> ECDSA P-256 verification (+ SHA-256)

RFC 8017 integer engine (shared with rsl-crypto-legacy)
  -> RSA public/private primitives
      -> RSASSA-PSS verification (+ SHA-256, MGF1)
```

Higher-level protocol key schedules consume these primitives without becoming dependencies of
this crate.

## 6. Algorithm lifecycle separation

`SecurityStatus` distinguishes recommended, legacy, broken, and educational-only algorithms.
The status describes lifecycle, not audit readiness. Historical primitives live in the separate
`rsl-crypto-legacy` package, are never included in a default protocol allowlist, and cannot be
selected by an implicit fallback. Protocol crates own the explicit act of enabling a historical
cipher suite; primitive crates only perform the named transformation.

## 7. Builders, lifecycle states, and fragmentation

Public lifecycle APIs use semantic stage types, not generic field wrappers. The AEAD record API
starts with `RecordBuilder`, advances to `RecordBuilderWithSequence`, and exposes `build_sealer`
or `build_opener` only on `ReadyRecordBuilder`. `RecordSealer::finish_to(self, sink)` consumes the
open writer, while `DataRecord` and `FinalRecord` make the authenticated end state explicit. The
stages therefore prevent invalid calls without asking users to manipulate typestate markers.

Callers provide arbitrary fragments through `RecordSealer::write_to`; output boundaries depend on
the concatenated byte stream and configured record size, not on call boundaries. Each completed
record is moved into a caller-selected fallible `RecordSink`. The sealer invalidates itself before
external sink code runs, so an error or caught panic cannot retry an already used nonce. The
collecting `write`/`finish` methods adapt the same state machine. Fixed-size algorithm blocks
remain internal to digest, MAC, cipher, and AEAD implementations. Protocol crates still define
their own semantic phases and wire encodings.
