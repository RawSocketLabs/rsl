# session/tls

A from-scratch TLS client stack, beginning with TLS 1.2 (RFC 5246). refcheck protocol name:
**`tls`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../../AGENTS.md) and protocol guide [`../../AGENTS.md`](../../AGENTS.md)
> (dual-use philosophy, the codec) also apply.

## Status — skeleton

Two pieces exist: the record-layer header codec, and the certificate module (peer chain plus
the server-verification policy). There is no handshake, no record protection and no
connection type yet, so the crate cannot open a TLS connection. [`DESIGN.md`](DESIGN.md)
records the plan, the decisions and the lower-layer gaps.

## Architecture

- `src/wire/` — RFC 5246 wire types on bnb, one type per file: `ContentType` (a `BitEnum` with
  an `Other` catch-all), `ProtocolVersion` (`{ major, minor }`, any pair round-trips) and
  `RecordHeader` (the 5-byte header; the fragment is not part of it). Codecs do no I/O and
  enforce no policy.
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
  feature), never from this crate.

## The dual-use rule here

- Wire types store every field verbatim. Decoding accepts unknown content types, any version
  pair and any length. `RecordHeader::MAX_PLAINTEXT_LEN` (2^14) and `MAX_CIPHERTEXT_LEN`
  (2^14 + 2048) are what a compliant sender honours for unprotected and protected records;
  the record layer (not the codec) enforces them on the guided path.
- `PeerChain` keeps the peer's bytes unparsed, so a chain `x509` rejects can still be read or
  pinned by the caller.
- `ServerVerification` has no default. `Unverified` is the explicit escape hatch; nothing in
  the crate may fall back to it silently.

## Testing

- **`contract`** — `tests/contract.rs`: record-header golden bytes laid out from RFC 5246
  §6.2.1.
- **`adversarial`** — `tests/adversarial.rs`: truncation, unknown values, arbitrary bytes.
- **`integration`** — `tests/integration.rs`: certificates built by `x509`, signed with
  `crypto` Ed25519 keys through `pki::issuance`, then parsed and verified through this crate.
  Rejections are pinned to the `pki::ErrorKind` that caused them, and each argument `verify`
  passes to `pki` (leaf, intermediates, purpose, key usage, name, time) has a test that fails
  without it.

Run everything: `cargo test -p tls`.
