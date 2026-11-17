# tls — design record

Decisions for `session/tls`. Numbered so reviews can cite them. Dated entries say what was
observed and how.

## 1. Why this crate exists

rsl has no TLS. The first consumers are the Sangfor VPN client protocols (aTrust and
EasyConnect), which need a TLS client that can do things
a general-purpose library does not offer: skip certificate verification, expose the server's
chosen `session_id` (EasyConnect derives its tunnel token from it), expose the peer's DER
chain (aTrust hashes it for its anti-MITM check), and, for the legacy tunnel, send a
ClientHello with a caller-chosen `session_id` over TLS 1.1 with RC4. TLS is built as its own
layer so that HTTP and the VPN protocols sit on it and none of them carries a private copy.

## 2. Layering

```
sangfor-*  http1        consumers
     \      /
       tls              this crate: wire codecs, handshake, record layer, connection
    /   |   \
 x509  pki  crypto      certificate syntax, path validation, primitives
   \    |
    asn1   bnb          DER, the wire codec
```

- **Certificates**: `x509` parses them and extracts public keys; `pki` validates paths,
  validity time, purpose and the server name. `tls` has no ASN.1 code. The touch points are:
  - the `Certificate` handshake message → `PeerChain` → `x509::Certificate::from_der`;
  - the server's key-exchange signature → `PeerChain::leaf_public_key` → `x509::PublicKey`;
  - the trust decision → `ServerVerification::verify` → `pki::PathValidator`
    (`Purpose::ServerAuth`, `dns_name`, caller-supplied time, and the `RequiredKeyUsage` the
    key exchange needs: a signature for ECDHE, key encipherment for static RSA, per RFC 5246
    §7.4.2);
  - later, client-certificate authentication, which also takes `x509` types.
- **Primitives**: `crypto` supplies ECDH (X25519, P-256), AES-GCM, HMAC, SHA-2 and RSA-PSS.
  Anything broken or historical (RC4, MD5, SHA-1, PKCS #1 v1.5) comes from `crypto-legacy`
  behind a `legacy` feature and is never offered by default.
- **Wire**: every message is a bnb type under `src/wire/`.

## 3. Decisions

1. **In-house, not a dependency.** A general-purpose TLS library has no TLS 1.1, RC4 or
   static-RSA key exchange and no way to set or read `session_id`, so EasyConnect would need
   a second stack anyway. This was argued from knowledge of such libraries, not checked
   against their source.
2. **Client only.** No consumer needs a server.
3. **Sans-I/O core.** The connection will be a state machine over byte buffers, as in
   `socks::v5::sansio`; blocking and async adapters are thin wrappers added when a consumer
   needs them. It reads no clock: the caller passes the time to `ServerVerification::Path`.
4. **TLS 1.2 first**, with `TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`. Observed 2026-11-16 with
   `openssl s_client` against the reference aTrust gateway's :443: TLS 1.2 negotiates that
   suite and returns a 32-byte `session_id`; TLS 1.3 is also accepted; TLS 1.1 and
   `TLS_RSA_WITH_AES_128_CBC_SHA` are refused. TLS 1.3 is deferred because nothing requires
   it and the token derivation needs a TLS 1.2 `session_id`.
5. **Verification is explicit.** `ServerVerification` has no default; `Unverified` must be
   chosen by name.
6. **The peer chain is kept as received.** `PeerChain` stores DER bytes and parses on demand,
   so an unverified connection still exposes a chain `x509` rejects. Under `Path` every
   certificate the peer sent must parse: one malformed spare certificate fails verification
   even when the leaf chains to an anchor without it. That fails closed and keeps the error
   attributable to a position.

## 4. Build order

1. Record header, peer chain, server verification — **done** (the skeleton).
2. Handshake wire types: ClientHello and ServerHello with extensions, Certificate (which
   yields `PeerChain`), ServerKeyExchange, ServerHelloDone, ClientKeyExchange, Finished, and
   alerts.
3. Key schedule and record protection: the TLS 1.2 PRF on HMAC-SHA-256, AES-128-GCM records.
4. The sans-I/O client handshake, including write half-close (send `close_notify`, keep
   reading), which the aTrust TCP tunnel needs.
5. A blocking adapter, then an interoperability test against a local server.
6. `legacy` feature: TLS 1.1 with `TLS_RSA_WITH_RC4_128_SHA` and ClientHello hooks. Deferred
   with EasyConnect.

## 5. Findings for the layers below

- **`x509`/`pki`: no PKCS #1 v1.5 certificate signatures.** `x509::SignatureAlgorithm` covers
  ECDSA P-256/P-384, Ed25519, Ed448 and RSA-PSS-SHA-256. Most public RSA certificates are
  signed with `sha256WithRSAEncryption`, so `ServerVerification::Path` will fail on those
  chains. The failure surfaces as `PathNotFound`, not `UnsupportedAlgorithm`: path building
  treats a signature it cannot check as "not this issuer" and discards the cause. The
  Sangfor consumers use `Unverified`, so this does not block them. Whether to accept v1.5
  certificate signatures, and whether to surface the cause, are `pki` decisions.
- **`pki`: DNS names only.** Service-identity matching reads `dNSName` entries, so a gateway
  addressed by IP literal can never pass `Path`; `ServerVerification::Path` therefore takes a
  `dns_name` and has no IP form.
- **`crypto`: PKCS #1 v1.5 signatures live in `crypto-legacy`.** With ECDHE-RSA the server
  signs its key exchange. Step 4 will offer the RSA-PSS schemes in `signature_algorithms`.
  Observed 2026-11-16 with `openssl s_client -sigalgs rsa_pss_rsae_sha256`: the reference
  gateway's :443 signs with `rsa_pss_rsae_sha256` in TLS 1.2 and accepts X25519 and P-256,
  so core `crypto` is enough there. A gateway that insists on v1.5 would need the `legacy`
  feature.
- **`crypto`: no operating-system random source.** `RandomSource` is a trait with test-only
  implementations; nothing in the workspace reads the OS generator. The handshake needs
  entropy for the client random and the ECDHE key. The sans-I/O core takes a `RandomSource`
  from the caller, so this blocks only the blocking adapter (step 5), which needs an
  OS-backed source in `crypto`.

### Readiness check, 2026-11-16

A throwaway test (not kept) ran the operations the first suite needs against an independent
implementation, Python `cryptography` 50.0.0, with freshly generated inputs. All agreed:
RSA-PSS-SHA-256 verification with the key taken from a PKCS #1 v1.5-signed certificate through
`PeerChain::leaf_public_key`; X25519 and ECDH P-256 shared secrets; AES-128-GCM seal and open
with associated data; HMAC-SHA-256; and a cloned SHA-256 state for the transcript hash. The
reference gateway's real three-certificate chain parses through `x509`, and its RSA keys are
extracted; only the certificates' signature algorithm is unsupported, as recorded above. This
shows the happy paths interoperate. It is not a security qualification of `crypto`.

## 6. Non-goals

A server role, TLS 1.3, session resumption, DTLS, the GM/TLCP variant, `no_std` (the layers
below are `no_std + alloc`, so it stays possible, but no consumer needs it).

## 7. Risks

- **Scope is set by one gateway.** The suite and versions in decision 4 were observed on :443
  only. The aTrust tunnel nodes listen on other hosts and ports that are listed only after
  login, so their TLS has not been observed.
- **The layers below are early.** `crypto` describes itself as an early implementation with no
  audit and no measured timing analysis; `x509` and `pki` are unaudited. A connection made with
  this stack inherits that, whatever `tls` itself does.
- **A TLS stack is a large security surface.** Each step lands with RFC vectors, adversarial
  tests and a fuzz target for the handshake parser before a consumer depends on it.
