# Choosing cryptographic algorithms

This guide maps a protocol task to an implemented primitive. It does not teach the algorithms;
follow each module link for mechanics and `STANDARDS.md` for controlling publications and test
evidence.

> `rsl-crypto` is not independently audited production cryptography. `Recommended` is a project
> lifecycle label, not a deployment approval or constant-time claim. See
> [`SIDE-CHANNELS.md`](SIDE-CHANNELS.md).

## Task to family

| Task | Choose | Do not substitute | Mechanics and citations |
| --- | --- | --- | --- |
| Confidentiality **and** integrity for messages, records, or packets | An AEAD: AES-GCM or ChaCha20-Poly1305 | A raw block or stream cipher | [`aead`](src/aead.rs); [GCM](src/aead/gcm/mod.rs) / [ChaCha20-Poly1305](src/aead/chacha20poly1305/mod.rs); [ledger](STANDARDS.md#aes-128-gcm-coverage) |
| Bounded-memory confidentiality and integrity for one large byte stream | The generic AEAD record sealer/opener around a selected AEAD, when its local record contract fits the format | Splitting raw AES blocks, or encrypting chunks with one repeated nonce | [AEAD records](src/aead/record.rs); [ledger](STANDARDS.md#generic-aead-record-source-baseline) |
| Establish a secret with a peer | X25519, X448, or validated ECDH | An unauthenticated agreement as proof of peer identity | [`agreement`](src/agreement.rs); [X25519](src/agreement/x25519/mod.rs) / [X448](src/agreement/x448/mod.rs) / [P-256](src/agreement/ecdh_p256/mod.rs) / [P-384](src/agreement/ecdh_p384/mod.rs); [ledger](STANDARDS.md#x25519-source-baseline) |
| Authenticate a peer or signed object | A protocol-selected signature scheme | A MAC when verifiers do not share one secret | [`signature`](src/signature.rs); [Ed25519](src/signature/ed25519/mod.rs) / [ECDSA P-256](src/signature/ecdsa_p256/mod.rs) / [RSA-PSS](src/signature/rsa_pss/mod.rs); [ledger](STANDARDS.md#ed25519-source-baseline) |
| Derive traffic keys, IVs, or purpose-separated subkeys | HKDF with the suite's hash | Hashing concatenated inputs and slicing the digest | [`kdf`](src/kdf.rs); [HKDF-SHA-256](src/kdf/hkdf/sha256/mod.rs) / [HKDF-SHA-384](src/kdf/hkdf/sha384/mod.rs); [ledger](STANDARDS.md#hkdf-sha-256-source-baseline) |
| Authenticate with an already shared secret | HMAC; Poly1305 only where a construction supplies a one-time key | An unkeyed digest | [`mac`](src/mac.rs); [HMAC-SHA-256](src/mac/hmac/sha256/mod.rs) / [Poly1305](src/mac/poly1305/mod.rs); [ledger](STANDARDS.md#hmac-sha-256-source-baseline) |
| Compute a protocol-defined fingerprint or hash commitment | The digest named by that protocol, over a canonical and domain-separated encoding | A digest match as proof of authorship | [`digest`](src/digest/mod.rs); [SHA-256](src/digest/sha2/sha256/mod.rs) / [SHA-384](src/digest/sha2/sha384/mod.rs) / [SHA-3](src/digest/sha3/mod.rs); [ledger](STANDARDS.md#sha-256-source-baseline) |
| **Never:** protect messages with raw AES or ChaCha20 | Use an AEAD profile | Raw AES supplies no mode, nonce, or authentication; raw ChaCha20 supplies no authentication | [`cipher`](src/cipher.rs); [AES](src/cipher/aes/mod.rs) / [ChaCha20](src/cipher/chacha20/mod.rs); [ledger](STANDARDS.md#aes-128-source-baseline) |
| **Never:** use a digest as a MAC | Use HMAC, or a specified one-time Poly1305 construction | `SHA-256(secret || message)` and bare digests | [SHA-256 mistakes](src/digest/sha2/sha256/mod.rs) / [HMAC](src/mac/hmac/sha256/mod.rs); [ledger](STANDARDS.md#hmac-sha-256-notation-mapping-and-coverage) |
| **Never:** use raw ECDH/X25519/X448 output directly as a traffic key | Feed the shared secret and required public context into the protocol's KDF | Truncation, padding, or direct use of `Z` | [`agreement`](src/agreement.rs) / [HKDF](src/kdf/hkdf/sha256/mod.rs); [ledger](STANDARDS.md#ecdh-p-256-source-baseline) |
| **Never:** select PKCS #1 v1.5 for a new design | Use a contemporary scheme; permit v1.5 only behind an explicit legacy interoperability policy | Default negotiation or silent fallback to the legacy package | [legacy PKCS #1 v1.5](../crypto-legacy/src/rsa/pkcs1v15.rs) / [RSA-PSS](src/signature/rsa_pss/mod.rs); [current ledger](STANDARDS.md#rsa-source-baseline) / [legacy ledger](../crypto-legacy/STANDARDS.md#rsa-pkcs-1-v15) |

## Within a family

### Authenticated encryption

| Choice | Select when | Why / constraint | Mechanics and citations |
| --- | --- | --- | --- |
| AES-GCM | A FIPS/NIST or existing protocol profile selects AES, or the deployed backend has verified effective AES acceleration | Common hardware-accelerated path. A nonce repeated under one key destroys the profile's security. | [GCM](src/aead/gcm/mod.rs); [ledger](STANDARDS.md#aes-128-gcm-coverage) |
| ChaCha20-Poly1305 | The target is software-only or lacks effective AES acceleration, and the protocol selects the IETF construction | Portable software-oriented path. Nonce reuse is equally forbidden. | [ChaCha20-Poly1305](src/aead/chacha20poly1305/mod.rs); [ledger](STANDARDS.md#chacha20-poly1305-and-aead_chacha20_poly1305-source-baseline) |
| AES-128-GCM | The suite targets roughly 128-bit strength | Smaller key and the mandatory TLS 1.3 AES-GCM suite pairing with SHA-256. | [AES-128-GCM](src/aead/gcm/mod.rs); [ledger](STANDARDS.md#aes-128-gcm-coverage) |
| AES-256-GCM | The selected profile requires AES-256, especially the TLS `SHA384` family | TLS pairs it with HKDF-SHA-384. A 256-bit AES key does not raise a suite above its weakest component. | [AES-256-GCM API](src/aead/gcm/api256.rs); [ledger](STANDARDS.md#aes-256-and-aes-256-gcm) |

This crate currently exposes readable reference paths, not an advertised hardware-accelerated
backend. Use the distinction above for protocol and deployment selection, then measure the actual
backend rather than assuming acceleration from the algorithm name.

`Aead::seal` accepts any supported contiguous byte slice and performs internal AES/ChaCha block
processing itself. Use [`aead::record`](src/aead/record.rs) only for bounded incremental input or
when the `aead-record/v1` data/final contract is explicitly part of the format; it is not TLS or
SSH record protection.

### Key agreement

| Choice | Approximate tier | Select when | Mechanics and citations |
| --- | --- | --- | --- |
| X25519 | 128 bit | Simplicity and broad modern protocol interoperability are primary | [X25519](src/agreement/x25519/mod.rs); [ledger](STANDARDS.md#x25519-source-baseline) |
| ECDH P-256 | 128 bit | A NIST/FIPS profile, certificate ecosystem, or existing wire format requires `secp256r1` | [ECDH P-256](src/agreement/ecdh_p256/mod.rs); [ledger](STANDARDS.md#ecdh-p-256-source-baseline) |
| ECDH P-384 | 192 bit | A NIST/FIPS profile requires a higher tier and pairs it with SHA-384 | [ECDH P-384](src/agreement/ecdh_p384/mod.rs); [ledger](STANDARDS.md#p-384-ecdh-p-384-and-ecdsa-p-384sha-384) |
| X448 | 224 bit | The protocol explicitly selects the higher curve448 tier and both peers implement it | [X448](src/agreement/x448/mod.rs); [ledger](STANDARDS.md#x448) |

All four produce an unauthenticated shared secret. Authentication, transcript binding, and the
KDF remain separate protocol operations. A NIST/FIPS algorithm choice does not make this
unvalidated crate a validated cryptographic module.

### Signatures

| Choice | Select when | Why / constraint | Mechanics and citations |
| --- | --- | --- | --- |
| Ed25519 | The protocol accepts raw EdDSA keys and signatures | Deterministic signing with no caller-managed per-signature nonce; usually the simplest new identity path | [Ed25519](src/signature/ed25519/mod.rs); [ledger](STANDARDS.md#ed25519-source-baseline) |
| ECDSA P-256 / P-384 | NIST/FIPS or X.509/TLS/SSH compatibility requires these curves | Existing certificate and protocol ecosystem. Signing here uses RFC 6979, so callers do not supply `k`; DER and transcript encodings remain protocol-owned. | [P-256](src/signature/ecdsa_p256/mod.rs) / [P-384](src/signature/ecdsa_p384/mod.rs); [ledger](STANDARDS.md#ecdsa-p-256-source-baseline) |
| Ed448 | Both peers explicitly support the 224-bit EdDSA tier | Deterministic higher-tier option with thinner interoperability than Ed25519 | [Ed448](src/signature/ed448/mod.rs); [ledger](STANDARDS.md#ed448) |
| RSA-PSS/SHA-256 verification | An existing RSA certificate path requires it | Certificate-verification compatibility only here: this crate does not expose PSS signing. The expected salt length is part of the verifier profile. | [RSA-PSS](src/signature/rsa_pss/mod.rs); [ledger](STANDARDS.md#rsassa-pss-source-baseline) |

### Digests, XOFs, and derivation

| Choice | Select when | Why / constraint | Mechanics and citations |
| --- | --- | --- | --- |
| SHA-256 | The protocol targets the 128-bit collision-resistance tier or names a `SHA256` suite | Broad protocol pairing; fixed 32-byte output | [SHA-256](src/digest/sha2/sha256/mod.rs); [ledger](STANDARDS.md#sha-256-source-baseline) |
| SHA-384 | The protocol targets the 192-bit collision-resistance tier, P-384, or a TLS `SHA384` suite | Fixed 48-byte output; its truncated SHA-512 state does not permit ordinary state-based length extension | [SHA-384](src/digest/sha2/sha384/mod.rs); [ledger](STANDARDS.md#sha-384-hmac-sha-384-and-hkdf-sha-384) |
| SHA3-256 | The controlling profile requires FIPS 202 or a sponge rather than SHA-2 | Fixed 32-byte output; not a drop-in replacement where a protocol names SHA-256 | [SHA-3](src/digest/sha3/mod.rs); [ledger](STANDARDS.md#sha3-256-and-shake256-source-baseline) |
| SHAKE256 | The protocol requires an extendable-output function or Ed448's exact SHAKE profile | The caller selects output length; domain and length must come from the protocol | [SHAKE256](src/digest/sha3/mod.rs); [ledger](STANDARDS.md#sha3-256-and-shake256-source-baseline) |
| HKDF-SHA-256 / HKDF-SHA-384 | Deriving one or more keys from shared or nonuniform secret material | Extract handles input keying material; Expand uses `info` for purpose separation. Raw hashing provides neither contract. | [HKDF-SHA-256](src/kdf/hkdf/sha256/mod.rs) / [HKDF-SHA-384](src/kdf/hkdf/sha384/mod.rs); [ledger](STANDARDS.md#hkdf-sha-256-source-baseline) |

Length extension is a construction concern, not a reason to improvise authentication or key
derivation. Never replace HMAC or HKDF with a raw digest construction, regardless of digest
family.

### Shared-key authentication

| Choice | Select when | Constraint | Mechanics and citations |
| --- | --- | --- | --- |
| HMAC-SHA-256 / HMAC-SHA-384 | A reusable shared authentication key and a protocol-defined full-tag profile are available | Match the suite hash; truncation needs its own protocol policy | [HMAC-SHA-256](src/mac/hmac/sha256/mod.rs) / [HMAC-SHA-384](src/mac/hmac/sha384/mod.rs); [ledger](STANDARDS.md#hmac-sha-256-source-baseline) |
| Poly1305 | A construction derives a fresh one-time key for exactly one message | Never reuse a Poly1305 key; prefer the complete ChaCha20-Poly1305 AEAD where applicable | [Poly1305](src/mac/poly1305/mod.rs); [ledger](STANDARDS.md#chacha20-poly1305-and-aead_chacha20_poly1305-source-baseline) |

## What protocols pair

Match security levels across the suite: the weakest agreement, signature, digest/KDF, symmetric
algorithm, parameter size, or key-management rule bounds the result. Do not spend for a higher
tier in one component while leaving another at a lower tier unless the controlling protocol fixes
that pairing.

### TLS 1.3

TLS 1.3 negotiates the cipher suite, key-share group, and signature scheme independently. Its
cipher-suite name selects only an AEAD and the hash used by HKDF and transcript hashing.

| TLS choice | Primitive pairing | Selection note | Mechanics and protocol citations |
| --- | --- | --- | --- |
| `TLS_AES_128_GCM_SHA256` | AES-128-GCM + HKDF/HMAC/SHA-256 | Baseline 128-bit family; mandatory-to-implement TLS suite | [GCM](src/aead/gcm/mod.rs) / [HKDF-SHA-256](src/kdf/hkdf/sha256/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `TLS_AES_256_GCM_SHA384` | AES-256-GCM + HKDF/HMAC/SHA-384 | The standardized AES-256 / `SHA384` pairing | [AES-256-GCM](src/aead/gcm/api256.rs) / [HKDF-SHA-384](src/kdf/hkdf/sha384/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `TLS_CHACHA20_POLY1305_SHA256` | IETF ChaCha20-Poly1305 + HKDF/HMAC/SHA-256 | Software-oriented AEAD alternative in the 128-bit family | [ChaCha20-Poly1305](src/aead/chacha20poly1305/mod.rs) / [HKDF-SHA-256](src/kdf/hkdf/sha256/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| Key-share groups | `x25519` or `secp256r1` at the 128-bit tier; `secp384r1` at 192 bits; `x448` at about 224 bits | A cipher suite does not select the group | [X25519](src/agreement/x25519/mod.rs) / [P-256](src/agreement/ecdh_p256/mod.rs) / [P-384](src/agreement/ecdh_p384/mod.rs) / [X448](src/agreement/x448/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| Signature schemes | `ed25519`, `ed448`, `ecdsa_secp256r1_sha256`, `ecdsa_secp384r1_sha384`, and the implemented `rsa_pss_*_sha256` verification profile | Certificate and `CertificateVerify` capabilities are signaled separately. This crate's RSA-PSS path is verification-only. | [EdDSA](src/signature/ed25519/mod.rs) / [ECDSA](src/signature/ecdsa_p256/mod.rs) / [RSA-PSS](src/signature/rsa_pss/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |

### SSH

| SSH choice | Primitive pairing | Selection note | Mechanics and protocol citations |
| --- | --- | --- | --- |
| `curve25519-sha256` | X25519 + SHA-256 exchange hash/KDF | RFC 9142 says `SHOULD`; the raw X25519 output is encoded and hashed with the SSH transcript, not used as a key | [X25519](src/agreement/x25519/mod.rs) / [SHA-256](src/digest/sha2/sha256/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `ecdh-sha2-nistp256` / `ecdh-sha2-nistp384` | Validated ECDH + matching SHA-256 / SHA-384 | NIST-curve alternatives; match ECDSA and ECDH curve tiers when policy permits | [P-256](src/agreement/ecdh_p256/mod.rs) / [P-384](src/agreement/ecdh_p384/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `aes256-gcm@openssh.com` | AES-256-GCM with SSH packet framing and IV state | SSH derives a 12-byte initial IV, then increments its 64-bit invocation-counter field per packet | [AES-256-GCM](src/aead/gcm/api256.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `chacha20-poly1305@openssh.com` | Two ChaCha20 keys + Poly1305 + a 64-bit packet-sequence nonce | **Different construction:** it is not this crate's RFC 8439 [`ChaCha20Poly1305`](src/aead/chacha20poly1305/mod.rs), which has one key and a 96-bit nonce | [ChaCha20](src/cipher/chacha20/mod.rs) / [Poly1305](src/mac/poly1305/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `ssh-ed25519` | Pure Ed25519 over the SSH-specified signed bytes | Compact deterministic host/user signature path | [Ed25519](src/signature/ed25519/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |
| `ecdsa-sha2-nistp256` | ECDSA P-256/SHA-256 with SSH `mpint` signature framing | NIST/FIPS and existing-key interoperability path | [ECDSA P-256](src/signature/ecdsa_p256/mod.rs); [ledger](STANDARDS.md#protocol-selection-source-baselines) |

X448 and Ed448 form the standardized but rarely negotiated 224-bit tier (`curve448-sha512` and
`ssh-ed448` in SSH). RFC 9142 assigns curve448 `MAY` versus curve25519 `SHOULD`, and OpenSSH's
implementation index lists only the 25519 members. Treat the 448 tier as explicit-policy
interoperability, not an automatic upgrade. This crate maps no RSA-PSS profile to an SSH algorithm;
retain RSA-PSS here for existing certificate-verification paths only. See the
[protocol-source notes](STANDARDS.md#protocol-selection-source-baselines).

## Cross-cutting rules

| Rule | Required handling | Mechanics and citations |
| --- | --- | --- |
| AEAD nonce/IV uniqueness | Never repeat a nonce under one key. Random generation does not track reuse. TLS derives a static write IV and XORs it with the padded record sequence number; SSH AES-GCM derives an initial IV and increments its invocation counter. | [GCM](src/aead/gcm/mod.rs) / [ChaCha20-Poly1305](src/aead/chacha20poly1305/mod.rs); [primitive ledger](STANDARDS.md#aes-128-gcm-coverage) / [protocol ledger](STANDARDS.md#protocol-selection-source-baselines) |
| Generic AEAD record completion | `RecordSealer::write_to` moves completed records into a fallible `RecordSink`; `finish_to` consumes it. `RecordOpener::open_data_to` authenticates before moving plaintext into `RecordPlaintextSink`; `open_final_to` consumes it. Either sink failure invalidates the live stream state. A decoder must require exactly one valid final record. Fixed fields remain unique across streams under one key. | [AEAD records](src/aead/record.rs); [ledger](STANDARDS.md#generic-aead-record-source-baseline) |
| HKDF `salt` versus `info` | `salt` belongs to Extract and may strengthen/separate extraction; `info` belongs to Expand and binds purpose, direction, transcript, identities, and labels. Neither substitutes for the other. | [HKDF-SHA-256](src/kdf/hkdf/sha256/mod.rs); [ledger](STANDARDS.md#hkdf-sha-256-notation-mapping-and-coverage) |
| EdDSA contexts | Ed25519ctx uses nonempty `dom2` context separation; pure Ed25519 has no context. Ed448 always applies `dom4`, including an empty default context. Signer and verifier must select the same variant and exact protocol-defined context. | [Ed25519](src/signature/ed25519/mod.rs) / [Ed448](src/signature/ed448/mod.rs); [ledger](STANDARDS.md#ed25519-notation-mapping-and-coverage) |
| PSS salt length | The verifier receives the expected salt length from its profile; never infer it from the signature. The implemented TLS SHA-256 default is 32 bytes. | [RSA-PSS](src/signature/rsa_pss/mod.rs); [ledger](STANDARDS.md#rsassa-pss-source-baseline) |
| `SecurityStatus` | `Recommended` means permitted by the project's current standards baseline, not audited. `Legacy`, `Broken`, and `EducationalOnly` must never enter default negotiation. | [`SecurityStatus`](src/security.rs); [RSA lifecycle ledger](STANDARDS.md#rsa-source-baseline) / [legacy ledger](../crypto-legacy/STANDARDS.md) |

## What this crate does not decide

| Protocol decision | Owner | Primitive boundary and citations |
| --- | --- | --- |
| Negotiation, preference order, downgrade handling, and explicit legacy allowlists | TLS/SSH protocol state machine | [`SecurityStatus`](src/security.rs) supplies labels only; [protocol ledger](STANDARDS.md#protocol-selection-source-baselines) |
| Key generation policy, key lifetimes, rekey thresholds, protocol-specific sequence exhaustion, and destruction of caller-owned output | Protocol/key-management layer | [`RandomSource`](src/random.rs), secret owners, and the generic [record counter](src/aead/record.rs) expose narrow contracts; [GCM ledger](STANDARDS.md#aes-128-gcm-coverage) / [record ledger](STANDARDS.md#generic-aead-record-source-baseline) |
| Transcript construction, canonical wire encoding, identity binding, certificate parsing, and signature algorithm identifiers | Protocol and certificate codecs | [`agreement`](src/agreement.rs) and [`signature`](src/signature.rs) consume exact bytes/keys; [protocol ledger](STANDARDS.md#protocol-selection-source-baselines) |
| Replay detection, receive windows, packet/record ordering, and encryption activation state | Protocol connection state | [`aead`](src/aead.rs) authenticates one invocation only; [GCM ledger](STANDARDS.md#aes-128-gcm-coverage) |

Certificate parsing and path validation live in sibling [`rsl-x509`](../pki/x509/README.md) and
[`rsl-pki`](../pki/validation/README.md) crates. Negotiation, transcript binding, replay, and
record state belong in the sibling [`protocols`](../protocols/README.md) crates. A future TLS
or SSH crate should carry its own standards ledger for the complete construction and link back to
the primitive modules selected here.
