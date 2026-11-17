# session/tls

A from-scratch TLS client stack, beginning with TLS 1.2 (RFC 5246). refcheck protocol name:
**`tls`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../../AGENTS.md) and protocol guide [`../../AGENTS.md`](../../AGENTS.md)
> (dual-use philosophy, the codec) also apply.

## Status — dev (parts built, no connection yet)

Four pieces exist: the wire types (record header, alerts, and the handshake messages of an
ECDHE exchange), the certificate module (peer chain plus the server-verification policy), the
TLS 1.2 key schedule, and AES-128-GCM record protection. Nothing drives them yet: there is no
handshake state machine and no connection type, so the crate cannot open a TLS connection. [`DESIGN.md`](DESIGN.md)
records the plan, the decisions and the lower-layer gaps.

## Architecture

- `src/wire/` — TLS 1.2 wire types on bnb, one message or registry per file. Codecs do no I/O
  and enforce no policy.
  - Record layer: `RecordHeader` (the 5-byte header; the fragment is not part of it),
    `ContentType`, `ProtocolVersion`, `Alert`.
  - Handshake: `HandshakeHeader` (type and `u24` length), then one body type per message:
    `ClientHello`, `ServerHello`, `Certificate`, `ServerKeyExchange`, `ClientKeyExchange`,
    `Finished`. A body is decoded with `decode_exact` from exactly the bytes its header
    counts. `ServerHelloDone` has an empty body and so no type. `ChangeCipherSpec` is the
    single byte `1` and has no type either.
  - `ServerKeyExchange` and `ClientKeyExchange` are the ECDHE layouts (RFC 8422). Decode them
    only once the negotiated suite says the exchange is ECDHE.
  - Registries are `BitEnum`s with an `Other` catch-all. `CipherSuite` and `ExtensionType`
    name only what the crate acts on; everything else round-trips as `Other`.
  - `Extension` keeps its data as raw bytes. Its constructors build the compliant encodings a
    client sends; there are no typed decoders for extension data yet.
  - `list.rs` holds the `parse_with`/`write_with` pairs for vectors whose prefix counts bytes,
    for the hello's optional trailing extension block, and for a body that is all remaining
    bytes (`Finished`). The last two are sound only under `decode_exact` on one whole body;
    the file's module documentation says why. Both stand in for bnb features
    that do not exist yet (see `../../ROADMAP.md`, bnb co-evolution); use them, do not copy
    them, and delete them when bnb grows the directives.
- `src/schedule/` — `prf_sha256` (the TLS 1.2 PRF on HMAC-SHA-256) and `MasterSecret`, which
  owns everything derived from it: `derive` and `derive_extended` (RFC 7627), the `Finished`
  check value for either `ConnectionEnd`, and the AES-128-GCM `TrafficKeys`.
- `src/record/` — the record layer.
  - `WriteState` and `ReadState` are the guided path. Each owns its direction's protection
    and sequence number, so a sequence number is never used twice under a key; `protect`
    is the key change. `ReadState` also owns the receive buffer, bounded to one largest
    record, and refuses an oversized record as soon as its header arrives.
  - `Aes128GcmProtection` is one direction's AES-128-GCM protection and is stateless: the
    caller passes each record's sequence number, type and version. Used directly it is the
    raw path, as is `seal_with_explicit_nonce`.
  - Content rules (no empty handshake records, alert sizes, which types may interleave) are
    not the record layer's; the connection applies them.
  - A receiver answers `RecordError::RecordOverflow` with `record_overflow` and every other
    read error with a fatal `bad_record_mac` (RFC 5288 §3); the variants exist for
    diagnostics, not for the peer.
- `src/certificate/` — `PeerChain` (the DER certificates as received, leaf first),
  `ServerVerification` (`Path` or `Unverified`) and `CertificateError`.
- `lib.rs` and each `mod.rs` are facades: declarations and re-exports only.
- Imports are grouped under `// --- Standard library ---`, `// --- Workspace dependencies ---`
  and `// --- Internal modules ---`, as in `session/socks`.

## Certificates go through `x509` and `pki`

This is the crate's standing rule. TLS carries certificates; it does not interpret them.

- Parsing a certificate is `x509::Certificate::from_der`. Reading the server's key is
  `SubjectPublicKeyInfo::public_key`, which yields `x509::PublicKey`. Building or validating a
  path, checking validity time, purpose, key usage or the server name is `pki::PathValidator`.
  The handshake tells `ServerVerification::verify` which `pki::RequiredKeyUsage` the
  negotiated key exchange needs.
- Never add ASN.1 parsing, OID tables, name matching or signature-over-certificate checks
  here. When `x509` or `pki` cannot express something TLS needs, that is a finding for that
  crate: record it in `DESIGN.md` §5 and fix it there.
- The same holds one layer down: primitives come from `crypto` (and `crypto-legacy` behind a
  feature), never from this crate. The PRF and the record nonce and additional-data layouts
  are TLS constructions and live here; HMAC, SHA-256 and AES-GCM do not.
- Secrets are held in `crypto::SecretBytes`, which redacts `Debug` and zeroes on drop. That
  includes PRF intermediates, which are as secret as the PRF's output. Do not derive `Debug`
  on a type that holds key material any other way.

## The dual-use rule here

- Wire types store every field verbatim. Decoding accepts unknown content types, suites,
  groups, schemes and extensions, any version pair and any length; it rejects only bytes that
  cannot be framed (a list that overruns or underruns its length). `RecordHeader::MAX_PLAINTEXT_LEN` (2^14) and `MAX_CIPHERTEXT_LEN`
  (2^14 + 2048) are what a compliant sender honours for unprotected and protected records;
  the record layer (not the codec) enforces them on the guided path.
- `PeerChain` keeps the peer's bytes unparsed, so a chain `x509` rejects can still be read or
  pinned by the caller.
- `ServerVerification` has no default. `Unverified` is the explicit escape hatch; nothing in
  the crate may fall back to it silently.

## Testing

- **`support`** — `tests/support/mod.rs`: one real TLS 1.2 connection captured from OpenSSL
  (handshake messages, protected records and the key-log master secret), with the commands
  that produced it. Golden data: replace it only with a fresh capture.
- **`unit`** — inline: the PRF against `openssl kdf TLS1-PRF`, and the list helpers.
- **`contract`** — `tests/contract.rs`: bytes laid out by hand from the RFC structure
  definitions, and the captured handshake, which each message type must decode and re-encode
  byte for byte. The extension constructors are checked against the extensions OpenSSL sent.
  The key schedule must reproduce OpenSSL's two `Finished` values from the logged master
  secret, open its protected records, and reproduce one of them when sealing with its nonce.
- **`adversarial`** — `tests/adversarial.rs`: unknown values, lists whose lengths lie in each
  direction, every truncation of the captured messages; a protected record with any one bit
  flipped, or opened under another sequence number, type, version or key.
- **`integration`** — `tests/integration.rs`: certificates built by `x509`, signed with
  `crypto` Ed25519 keys through `pki::issuance`, then parsed and verified through this crate.
  Rejections are pinned to the `pki::ErrorKind` that caused them, and each argument `verify`
  passes to `pki` (leaf, intermediates, purpose, key usage, name, time) has a test that fails
  without it. It also verifies the captured handshake's key-exchange signature with the leaf
  key from `x509` and RSA-PSS from `crypto`, which proves the wire types preserve every byte
  the signature covers.

Run everything: `cargo test -p tls`.
