# Sangfor aTrust client protocol: clean-room specification

Status: draft 1, 2026-11-16. Scope: the **aTrust** (Sangfor SDP, "newer") client protocol only.
EasyConnect is specified separately; parts both protocols share are marked **[shared]**.

Provenance: written clean-room from the behaviour of the open-source client
`Mythologyli/zju-connect` at commit `8576e7a16076c4175d6dbe276573a353cf7260be`, with no code or
comments copied. Paths in `code font` are for traceability only. Statements without a marker
describe that client's behaviour. **(inferred)** marks deductions, and **open question** marks
unknowns. The golden vectors in §12 are *derived from the stated rule*: computed for this
document from the algorithms as written here, on inputs chosen for this document.

Conventions: all multi-byte integers on the wire are **big-endian** unless stated otherwise.
"hex" means base-16; **HEX** means uppercase hex. `‖` means concatenation.

---

## 0. Big picture

aTrust has two planes, and both run over TLS to the same gateway family:

1. **Control plane**: HTTPS/1.1 JSON API on the portal host (`server:port`, default 443). It
   handles login (multi-step, server-driven), anti-MITM checks, device reporting, resource policy
   download and session refresh. The output is a session cookie `sid`, a username and a policy
   document.
2. **Data plane**: TLS connections to **nodes** (gateways, default port **441**), listed in
   the policy and grouped into node groups. There are three uses:
   - **VIP fetch / tunnel session auth**: a SOCKS5-shaped handshake that authenticates with
     `sid` and returns the client's virtual IP (VIP).
   - **L3 tunnel**: one long-lived TLS connection per node group. It carries raw IP packets,
     each flow is authorised separately, and keepalive heartbeats run on it.
   - **TCP tunnel**: one TLS connection per proxied TCP stream, using a SOCKS5-shaped CONNECT
     with an embedded signed JSON auth blob. Optional "zero-RTT" framing applies.

```
            ┌──────────── control plane (HTTPS, portal :443) ────────────┐
 manifest → authConfig(+anti-MITM) → primary auth → reportEnv → secondary chain
          → onlineInfo → clientResource ──► policy (apps, node groups, DNS)
            └────────────────────────────────────────────────────────────┘
                                   │ sid, username, policy
            ┌──────────── data plane (TLS, nodes :441) ──────────────────┐
 node probe (TLS handshake RTT) → VIP fetch → L3 tunnel(s) per group
                                            → TCP tunnel per stream
 periodic: authConfig refresh (sid rotation), node re-probe, heartbeats
            └────────────────────────────────────────────────────────────┘
```

---

## 1. Component layer map

The layers are listed bottom-up. "Crate" is a proposed split. Each needed layer becomes its own
crate, as required. Crate names are placeholders. A dependency arrow points down.

| # | Component | What it does | Key wire/format facts | Generic? | Depends on | Proposed crate |
|---|---|---|---|---|---|---|
| L0 | Underlay dialer | TCP connect bound to the pre-VPN egress interface. It excludes the VIP, so tunnel sockets are never routed back into the TUN. | Platform socket options only. No protocol bytes. | generic **[shared]** | OS | existing transport / platform glue. Not part of the protocol crates. |
| L1 | TLS client | TLS 1.2/1.3 client with **no certificate verification**. It must expose the peer certificate chain (DER) to the layer above. | See §2.1: SNI, no ALPN, no tickets on tunnels, optional NSS key log | generic **[shared]** | L0, `crypto`, `x509` (parse only) | **`tls`** (new, generic) |
| L2 | HTTP/1.1 client codec | Request serialisation and response parsing: Content-Length and chunked, optional gzip, keep-alive optional | See §2.2 minimal subset | generic **[shared]** | L1 | **`http1`** (new, generic) |
| L3 | Cookie jar | Host-scoped cookie store. Handles Set-Cookie (incl. deletion), Cookie header emission, import/export for persistence | RFC 6265 subset, §2.3 | generic **[shared]** | none (pure) | **`http-cookie`** (new, generic) |
| L4 | URL/query + form codec | `application/x-www-form-urlencoded` encode with **keys sorted bytewise**; URL parse for redirect `Location` | §2.4 | generic | none | part of `http1` (module) |
| L5 | JSON | Parse/emit. Must (a) keep raw bytes of a sub-object, (b) keep number lexemes verbatim, (c) match some keys case-insensitively, (d) emit compact output in a fixed key order | §2.5 | generic | none | blessed external dep (no new crate) |
| L6 | TOTP | RFC 6238 code generation from a base32 secret | SHA-1, 6 digits, 30 s step **(inferred defaults)** | generic | `crypto` (HMAC-SHA1) | **`otp`** (new, small, generic) |
| L7 | Image header sniff | Read width/height from PNG/JPEG/GIF headers for the captcha payload | §5.7 | generic | none | module inside the auth crate, or a tiny generic crate |
| L8 | aTrust signing and anti-MITM primitives | Sign-key derivation, challenge check, canonical `mitmSig`, certificate digest, nonce LCG, request/response HMAC, RSA password wrapping | §3 (all constants) | **Sangfor-specific** | `crypto` (SHA-256, HMAC-SHA256, AES-128-CBC, RSA PKCS#1 v1.5, MD5) | **`sangfor-atrust-sig`** (pure, no I/O) |
| L9 | Control-plane API | Typed builders and parsers for every endpoint: common query params, headers, response envelope, error codes. Sans-I/O. | §4 | Sangfor-specific | L4, L5, L8 | **`sangfor-atrust-api`** |
| L10 | Auth state machine | Login orchestration: authConfig, primary method, reportEnv, secondary chain, onlineInfo. Also the captcha loop, already-logged-in short-circuit and refresh | §5. Drives a user-supplied prompt callback. | Sangfor-specific | L9 (+ L2/L3 via an HTTP transport trait), L6, L7 | **`sangfor-atrust-auth`** |
| L11 | Policy model and matcher | Parses `clientResource` into IP rules, domain rules, DNS pins, DNS servers and node groups. Answers interval lookups (first/last match). | §6 | Sangfor-specific parser, generic interval index **[shared matcher semantics]** | L5 | **`sangfor-atrust-policy`** |
| L12 | Data-plane wire codecs | Byte-exact encoders/decoders: session-auth handshake, VIP header, L3 frames (0x13–0x96), TCP tunnel handshake, TCP tunnel data frames | §7, §8.1, §9.1 | Sangfor-specific (SOCKS5-shaped) | `bnb`. May reuse `socks` address encoding. | **`sangfor-atrust-wire`** |
| L13 | L3 flow engine | Conntrack table, per-flow auth scheduler, retries and timeouts, pending-packet cache, TCP state TTLs, heartbeat policy, downstream packet re-assembly. Sans-I/O state machine. | §8.2–§8.6 | Sangfor-specific (conntrack is generic-ish) | L12, `ip`/`tcp`/`udp`/`icmp` codecs | **`sangfor-atrust-l3`** |
| L14 | TCP tunnel stream | Handshake driver plus a framed/raw byte-stream adapter with half-close | §9 | Sangfor-specific | L12, L1 | **`sangfor-atrust-tcp`** |
| L15 | Node selection | TLS-handshake RTT probing, WAN-then-LAN fallback, periodic re-probe | §10 | Sangfor-specific policy over a generic prober | L1, L11 | module of the client crate |
| L16 | Client lifecycle | Manifest/capabilities, resume vs login, persistence blob, VIP install, per-group L3 pools with reconnect, session refresh timer, best-node refresh and stale eviction, teardown | §11 | Sangfor-specific | everything above | **`sangfor-atrust-client`** |

Things that are **not** needed: HTTP/2, WebSocket, TLS client certificates, server certificate
validation, SM2/SM3/SM4 or TLCP (GM TLS), DTLS/UDP transport, IPv6 data path (parsed partially,
otherwise ignored), proxy CONNECT. See §13 on SM2/GM.

---

## 2. Generic lower layers: the minimum subset aTrust needs

### 2.1 TLS client (`tls`)

Requirements below come from the client's TLS configuration (`client/atrust/tls.go`,
`client/atrust/auth/auth.go`, `internal/ping/tcp.go`) and its tests (`client/atrust/tls_test.go`).

| Property | Control plane (HTTPS) | Node probe | Tunnels (VIP, L3, TCP) |
|---|---|---|---|
| Versions | **TLS 1.2 and 1.3** offered (TLS library defaults, minimum 1.2) | same | same; the tunnel profile pins no version either |
| Cipher suites / groups | Library defaults **(inferred)**: TLS 1.3 AES-128-GCM-SHA256, AES-256-GCM-SHA384, CHACHA20-POLY1305. TLS 1.2 ECDHE-{ECDSA,RSA} with AES-GCM / CHACHA20-POLY1305 (no CBC suites). Groups X25519 (plus the hybrid X25519MLKEM768 with recent library versions), P-256, P-384. | same | same |
| Certificate verification | **disabled** (accepts self-signed) | disabled | disabled, and nothing else inspects the certificate |
| Peer chain exposure | **required**: full peer chain as DER, for the anti-MITM certificate-identity check (§3.5) | not needed | not needed |
| SNI | host part of the URL. Not sent when the host is an IP literal (standard behaviour). | target host | host part of `host:port` |
| Session tickets / resumption | default (tickets allowed, but no session cache, so no resumption **(inferred)**) | default | **tickets disabled** (reported to be what the vendor client does) |
| ALPN | none offered **(inferred**: in this configuration the reference HTTP stack stays on HTTP/1.1) | none | none |
| Client certificate | none | none | none |
| Key logging | optional, NSS `SSLKEYLOGFILE` format (lines containing `CLIENT_…`) | optional | optional |
| Close | the TCP tunnel needs **write half-close** = send `close_notify` and keep reading | — | — |

Minimum viable TLS for interop: a TLS 1.2 client with ECDHE-RSA-AES128-GCM-SHA256 and
ECDHE-RSA-AES256-GCM-SHA384 on X25519/P-256, plus a TLS 1.3 client with AES-128-GCM over X25519.
That is **(inferred)** sufficient. The gateway's actual choice is an **open question** (§13).
The implementation must be able to send `close_notify` without closing the read side.

### 2.2 HTTP/1.1 client (`http1`)

Used only for the control plane. The minimal subset:

- **Request**: `GET` and `POST`. Origin-form target with query. Headers emitted: `Host` (with
  `:port` if not 443), `User-Agent`, `Content-Type` (POST), `Content-Length` (POST only),
  `Cookie`, and the aTrust headers in §4.2. The reference stack also sends
  `Accept-Encoding: gzip` automatically and transparently decompresses. An implementation
  should either **omit `Accept-Encoding`**, which is recommended and works in practice
  **(inferred)**, or support gzip decoding.
- **Response**: status line, headers (case-insensitive), body framed by `Content-Length`,
  `Transfer-Encoding: chunked`, or read-to-close. Bodies are small JSON documents, except for the
  captcha image (binary, a few KB).
- **Redirects**: by default 3xx responses are followed automatically, up to 10 hops (the
  reference HTTP library's default). For **two endpoints only** (CAS and OAuth2 callbacks, §5.4) redirects must **not**
  be followed: the client reads the `302` and its `Location` header. A redirect policy toggle
  per request is required.
- **Timeouts**: whole request ≤ **20 s**. The caller can cancel; cancellation must abort an
  in-flight request.
- **Connection reuse**: optional. Correctness does not depend on keep-alive. The reference stack
  uses ordinary HTTP/1.1 persistent connections: it sends no `Connection` header and reuses an
  idle connection for the next request whenever the previous response was read to its end, so a
  login normally travels over a single TLS connection. Nothing the client does relies on that;
  the session lives in the cookies. Whether a gateway ties any login state to the connection is
  unknown, and nothing suggests that it does **(inferred)**.
- No HTTP/2, no `Expect: 100-continue`, no proxies, no auth schemes.

Note: a GET has no body and carries no `Content-Length`. POST bodies always have a known
length.

### 2.3 Cookie jar (`http-cookie`)

- Scope: cookies are stored per **host** (registrable-domain logic is not needed: everything is
  one host, `server[:port]`). The `Domain` attribute is not relied on **(inferred)**.
- Set-Cookie handling: name/value, `Path`, `Max-Age` (a negative value or `0` deletes),
  `Expires` (past date deletes). Same name+path replaces the earlier cookie.
- Emission: `Cookie: n1=v1; n2=v2` for matching path. All aTrust cookies observed use `Path=/`.
- Import: on restore, cookies are inserted with `Path=/` for `(host, scheme)` pairs from the
  persistence blob (§11.3). An explicitly supplied `sid` is inserted **after** imported cookies
  and therefore wins.
- Export: list `(host, "https", name, value)` for the base host.
- The session identifier is the cookie named **`sid`**. Any response can carry a `Set-Cookie`
  that changes it, so read the current `sid` out of the jar only once the final response of an
  exchange has been processed.

### 2.4 Query and form encoding

`application/x-www-form-urlencoded`: keys **sorted ascending bytewise**, `k=v` joined by `&`,
space→`+`, reserved bytes percent-encoded with uppercase hex. The signed string in §3.7
depends on this exact form, which is why the order matters. The common query string (§4.1)
therefore always serialises as:

```
clientType=SDPClient&lang=en-US&platform=Linux
```

Endpoint-specific parameters merge into the same sorted set. When a key collides, the
endpoint's value **replaces** the shared value; it is not appended.

### 2.5 JSON requirements

- **Compact output, fixed field order.** Signed JSON (tunnel auth requests, §8.4, §9.2) is
  hashed over the exact bytes sent, so the emitter must produce a deterministic order.
  Reproduce the field orders given in this spec. Bodies that are not signed may use any order.
  Wherever the reference uses an unordered map, it emits keys sorted alphabetically.
- **String escaping matches the reference**: the reference escapes `<`, `>`, `&` as
  `<`, `>`, `&`, and U+2028/U+2029 as ` `/` `. This matters only if
  such characters appear in signed fields (usernames, domains) **(inferred:** the server
  verifies over the bytes it receives, so any escaping is fine as long as the signature is
  computed over the exact bytes sent).
- **Raw retention**: the `antiMITMAttackData` object must be kept as raw bytes for the
  canonicalisation in §3.3. The number lexemes must survive (`1`, not `1.0`).
- **Case-insensitive key matching on input**: the reference decoder matches keys
  case-insensitively. Policy JSON is known to work with both `ID` and `id`, `NodeGroupID` and
  `nodeGroupId`, and so on (§6). Implement case-insensitive lookup for the policy document at
  least.
- Unknown fields are ignored everywhere.

---

## 3. Sangfor-specific primitives (`sangfor-atrust-sig`)

Source: `client/atrust/auth/anti_mitm.go`, `client/atrust/auth/psw.go`,
`client/atrust/tcptunnel.go`, `client/atrust/client.go`.

### 3.1 Constants

| Symbol | Role | Value (ASCII) |
|---|---|---|
| `C_cert` | appended to a certificate's base64 text before hashing | `@~*&!()-` (8 bytes) |
| `C_chal` | mixed into the key material of the challenge check | `OrHWuJz7gku5awmVb5w1sKTmfeCWHmzokBxmn0sn0faIcv1G10PdrbbRGKBrrZ3m` |
| `C_key` | mixed into the sign key | `3uW5IEy8KwDaOMK8uw1TmNr50U3aK1Qdu8b6vopXxGstzan3AJXxVNR6piuKi5Nq` |

These three strings are embedded in the vendor client and are needed for interoperability.
Nonce generator parameters (§3.3): multiplier 48271, modulus 2147483647 (2³¹−1), discard
threshold 0x7FFFFF00.

### 3.2 Inputs: `antiMITMAttackData`

This object arrives inside the `authConfig` response at `data.antiMITMAttackData`.

| JSON key | Type | Meaning |
|---|---|---|
| `enable` | int | `1` = certificate-identity check active. Any other value means skip that stage. |
| `devicePubKeyMod` | string | Opaque string (hex modulus) used **as text** in key derivation |
| `devicePubKeyExp` | string | Opaque string used as text |
| `rsaCert` | string | base64(DER) of the expected RSA server certificate, or empty |
| `sm2encCert` | string | base64(DER) of the SM2 encryption cert, or empty. Never parsed, only hashed as text. |
| `challenge` | string | Challenge text |
| `encryptedChallenge` | string | **HEX** of AES-CBC(challenge) |
| `ticket` | string | Echoed in the anti-MITM request |
| `antiMITMRequest` | bool | When **false** (and enable == 1) the client performs the second-stage request (§3.6). See §13. |
| `mitmSig` | string | hex HMAC over the canonical string (§3.3) |

### 3.3 Algorithms

**Sign key** (`K`, 32 bytes). It is derived from every `authConfig` response. If the object is
absent, all three strings are empty and K is still derived:

```
F = SHA256( devicePubKeyMod ‖ devicePubKeyExp ‖ C_key )
S = SHA256( HEX(F) ‖ challenge )
K = F XOR S                      (bytewise, 32 bytes)
```

**Challenge check**:

```
M   = SHA256( devicePubKeyMod ‖ devicePubKeyExp ‖ C_chal )
key = M[0..16], iv = M[16..32]
ct  = AES-128-CBC-Encrypt(key, iv, PKCS7(challenge))
pass iff HEX(ct) == encryptedChallenge           (constant-time compare, exact string)
```

The check cannot pass when any of its four inputs (`devicePubKeyMod`, `devicePubKeyExp`,
`challenge`, `encryptedChallenge`) is empty.

**Canonical string for `mitmSig`.** The signed text is built from the object exactly as
received:

- It has one `name:value` item for every member, at any nesting depth, whose value is neither an
  object nor an array. Members named **`mitmSig`** are left out wherever they occur. A member
  that holds an object or array contributes no item of its own, only those of the scalars
  beneath it (elements of arrays included).
- Value text: a string contributes its decoded content, a number the digits as received (`1`
  stays `1`), a boolean `true` or `false`, and null the word `null`.
- Items are ordered by name, ascending bytewise, and joined with `&`.
- When one name occurs at several depths only one item survives, and the reference does not
  define which (**open question**; real payloads are flat, so this does not arise in practice
  **(inferred)**).

`expected = HEX(HMAC-SHA256(K, canonical))`. Compare with `upper(mitmSig)`. The check fails if
`mitmSig` is empty.

**Certificate digest**: `D(cert) = SHA256( base64std(DER) ‖ C_cert )` (raw 32 bytes).
The expected digests are computed on the **base64 text exactly as received** in
`rsaCert`/`sm2encCert`, not re-encoded.

**Nonce**: 32 bytes, sent as 64 **HEX** characters, taken from one generator shared by the
whole process: the Park–Miller "minstd" recurrence x ← 48271·x mod (2³¹−1). The seed is the Unix
time in seconds reduced mod 2³¹−1, with 1 used in place of 0. Each new x yields the byte
(x−1) mod 256, except that an x with x−1 ≥ 0x7FFFFF00 yields nothing and the generator simply
advances. The server only echoes the nonce back; nothing depends on it being unpredictable.

**HMAC signature** (anti-MITM and tunnel requests): `HEX(HMAC-SHA256(key, message))`, 64
uppercase characters.

### 3.4 Password / challenge-response encryption

- RSA public key from `authConfig`: modulus = `data.pubKey` parsed as **hex**, exponent =
  `data.pubKeyExp` parsed as **decimal** (must be > 0).
- Plaintext = UTF-8 `secret ‖ "_" ‖ antiReplayRand`.
- Split the plaintext into chunks of `k − 11` bytes (k = modulus length in bytes). Encrypt each
  chunk with RSAES-PKCS1-v1_5 (random padding). Concatenate the ciphertext blocks (each exactly
  k bytes). Output **lowercase** hex.
- Always encrypt with the most recently received `antiReplayRand`. The E5 reply can replace the
  value authConfig supplied, and E18 answers code `75500000` when an outdated value was used.

### 3.5 Certificate-identity check (stage 3 of anti-MITM)

This check runs only when `enable == 1`. Fail if there are no peer certificates. Fail if both
`rsaCert` and `sm2encCert` are empty. Pass if **any** certificate in the peer chain has `D(cert)`
equal to the digest of `rsaCert` or of `sm2encCert`. Otherwise fail and report the leaf's
digest.

### 3.6 Second-stage anti-MITM request

See §4.3, row E3. The response is accepted only if all of the following hold:

- `data.nonce` equals the nonce that was sent.
- `code` is neither `10000004` (the session is unknown to the server) nor `10000008` (the server
  itself judged the connection intercepted).
- `upper(X-Response-Sig header)` equals `HEX(HMAC-SHA256(K, raw_response_body))`.

### 3.7 `X-Request-Sig` on reportEnv

`HEX(HMAC-SHA256(K, path ‖ "?" ‖ query ‖ body))`, where path is `/controller/v1/public/reportEnv`,
query is the exact sent query string (§2.4), and body is the exact JSON bytes sent. K comes from
the most recent `authConfig`.

### 3.8 Client identifiers

| Identifier | Generation | Where used |
|---|---|---|
| Device ID | 16 random bytes → 32 **lowercase** hex. Persisted. | `x-sdp-env`, reportEnv, tunnel auths, anti-MITM `mobileId` |
| MAC address list | One random MAC with the first octet `& 0xFE \| 0x02` (locally administered, unicast), lowercase `aa:bb:…`. Persisted. | reportEnv |
| Connection ID | `HEX(MD5(deviceId))` ‖ `-` ‖ decimal **unix microseconds** at setup | tunnel auths |
| Tunnel sign key | 32 random bytes as 64 **HEX** chars (or user-configured). Hex-decoded and used as the HMAC key for `xRequestSig`. | §8.4, §9.2 |
| Trace ID | 8 random lowercase hex chars per request (non-cryptographic) | `x-sdp-traceid` |
| RID | base64std(`host` or `host:port`, exactly as used in the base URL) | `x-sdp-rid` |
| Env header | base64std(`{"deviceId":"<deviceId>"}`) | `x-sdp-env` |
| Process fingerprint (`procHash`) | `HEX(SHA256(process path))` | tunnel auths |

The tunnel sign key is **never sent to the server** by this client. Whether the gateway
verifies `xRequestSig` is an **open question** (§13). Interop works with a random key
**(inferred** from the design).

---

## 4. Control-plane API (`sangfor-atrust-api`)

Base URL: `https://<server>` if the port is 443, else `https://<server>:<port>`. The same
string, without the scheme, is the "base host" used for redirect validation, RID and cookie
scope.

### 4.1 Common query parameters

Every `/passport/…` and `/controller/…` request carries `clientType=SDPClient`,
`platform=Linux`, `lang=en-US` (sorted, §2.4), plus the endpoint-specific parameters. The
exceptions are `/public/manifest` and the CAS/OAuth2 callback URLs, which carry none.

### 4.2 Common headers

| Header | Value | Sent on |
|---|---|---|
| `User-Agent` | `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) aTrustTray/2.4.10.50 Chrome/83.0.4103.94 Electron/9.0.2 Safari/537.36 aTrustTray-Linux-Plat-Ubuntu-x64 SPCClientType` | all |
| `x-csrf-token` | latest CSRF token from authConfig (empty string before the first one) | all except manifest and checkCode |
| `x-sdp-traceid` | §3.8 | all except manifest and checkCode |
| `x-sdp-rid` | §3.8 | authConfig, antiMITMRequest |
| `x-sdp-env` | §3.8 | psw, sendSms, smsCheckCode, token, challenge, enhanced steps |
| `Content-Type` | `application/json;charset=utf-8` (JSON bodies). `application/json` (antiMITMRequest only). `application/x-www-form-urlencoded` (SMS check, mode A). | POST |
| `X-Request-Sig` | §3.7 | reportEnv |
| `Accept` | `image/webp,image/apng,image/*,*/*;q=0.8` | checkCode |

The table is complete. A request carries `Host`, `User-Agent`, the rows above that apply to it,
`Cookie` when the jar holds cookies for the host, and `Content-Length` on a POST. The reference's
HTTP stack adds `Accept-Encoding: gzip` (§2.2) and nothing else, except a `Referer` on a request
it issues itself while following a redirect. In particular no request the client builds has an
`Origin`, `Referer`, `Accept-Language` or `Connection` header, and none but checkCode has
`Accept`. For the password path that gives:

| Request | Headers besides `Host`, `User-Agent`, `Cookie`, `Content-Length`, `Accept-Encoding` |
|---|---|
| E1 | none |
| E2 | `x-csrf-token`, `x-sdp-rid`, `x-sdp-traceid` |
| E5 | `Content-Type`, `x-csrf-token`, `x-sdp-env`, `x-sdp-traceid` |
| E10 | `Content-Type`, `x-csrf-token`, `x-sdp-traceid`, `X-Request-Sig` |
| E11, E22 | `x-csrf-token`, `x-sdp-traceid` |
| E23 | `Content-Type`, `x-csrf-token`, `x-sdp-traceid` |

`x-csrf-token` is sent even while the token is empty, as a header with an empty value. The
`x-…` names are written in lower case here. On the wire the reference's HTTP stack capitalises
each hyphen-separated word (`X-Csrf-Token`, `X-Sdp-Traceid`, `X-Sdp-Rid`, `X-Sdp-Env`). HTTP/1.1
field names are case-insensitive, so either spelling should do **(inferred)**.

### 4.3 Endpoint catalogue

All responses are JSON envelopes `{"code":int,"message":string,"data":{…}}` unless noted.
`code == 0` means success. HTTP status is ignored except where stated.

| # | Method, path | Extra query | Body | Response fields used |
|---|---|---|---|---|
| E1 | `GET /public/manifest` | none | none | Must be HTTP 200 and `code == 0`. `data.capacities.zeroRTT.version` (string), `data.options.tun0rtt.enable` (bool). The whole raw body is persisted. |
| E2 | `GET /passport/v1/public/authConfig` | `mod=1` (after CAS/OAuth2); `needTicket=1` (login paths); `needTicket=0` (refresh only) | none | `data.csrfToken` or `data.security.csrfToken` (the former unless it is empty, else the latter); `data.pubKey`, `data.pubKeyExp`, `data.antiReplayRand`, `data.antiMITMAttackData`, `data.isLogin` (int), `data.authServerInfoList[]` = {`loginDomain`,`authId`,`authType`,`authName`,`subType`,`loginUrl`}. HTTP 401/403 → session invalid. |
| E3 | `POST /controller/v1/public/antiMITMRequest` | `mobileId=<deviceId>` if known | literal bytes, see below | `data.nonce`, `data.antiMITMEnable`, header `X-Response-Sig` |
| E4 | `GET /passport/v1/public/checkCode` | `rnd=<unix ms>` | none | raw image bytes (PNG/JPEG/GIF) |
| E5 | `POST /passport/v1/auth/psw` | none | `{"graphCheckCode"?:S,"password":HEX_lower,"rememberPwd":"0","username":"<user>@<domain>"}` | `data.ticket`, `data.graphCheckCodeEnable` (int), `data.nextService`, `data.antiReplayRand` |
| E6 | `POST /passport/v1/public/sendSms` | none | `{"graphCheckCode":S,"phone":"<phone>@<domain>"}` (graphCheckCode always present, may be `""`) | `data.graphCheckCodeEnable`, `data.tips`, `data.interval` (string) |
| E7 | `POST /passport/v1/auth/smsCheckCode` | none | `{"code":S,"graphCheckCode"?:S,"phone":"<phone>@<domain>"}` | `data.ticket`, `data.graphCheckCodeEnable` |
| E8 | `GET <base>/passport/v1/auth/cas?sfDomain=<d>&ticket=<t>` (or a user-pasted callback URL) | as shown; **no common params**; **do not follow redirects** | none | must be **302**. `Location` → portal ticket (§5.4) |
| E9 | `GET <base>/passport/v1/auth/httpsOauth2?code=<c>&sfDomain=<d>&state=null` (or pasted) | same | none | same as E8 |
| E10 | `POST /controller/v1/public/reportEnv` | none | `{"deviceId":D,"env":{"endpoint":{"device":{"type":"browser"},"device_id":D,"mac_addresses":[…]}},"ticket":T}` | `code` |
| E11 | `GET /passport/v1/auth/authCheck` | none | none | `data.nextService`, `data.nextServiceList[]` = {`authId`,`authType`,`subType`} |
| E12 | `GET /passport/v1/public/phoneNumber` | `authId` if known | none | `data.phoneNumber` (string, array of strings, or null), `data.maskIdentifierValue` (display only) |
| E13 | `GET /passport/v1/auth/sms` | `action=sendsms`; mode B adds `authId=<id>&isPrevEffect=0&taskId=` | none | `code` ∈ {0, **75500401**} counts as success. `message`, `data.tips` |
| E14 | `POST /passport/v1/auth/sms` | `action=checkcode` | mode A (form): `code=<c>&skipSecondaryAuth=0\|1`. Mode B (JSON): `{"authId":A,"code":C,"isPrevEffect":false,"skipSecondaryAuth":"0"\|"1","taskId":""}` | next-step fields (E11 shape) |
| E15 | `POST /passport/v1/auth/customSms` | `action=sendcustomsms` | `{"isPrevEffect":"0","taskId":""}` | `data.tips` |
| E16 | `POST /passport/v1/auth/customSms` | `action=checkcustomcode` | `{"isPrevEffect":false,"customCode":C,"skipSecondaryAuth":"0"\|"1","taskId":""}` | next-step fields |
| E17 | `POST /passport/v1/auth/token` | none | TOTP: `{"action":"auth","isPrevEffect":false,"skipSecondaryAuth":0\|1,"totpToken":C,"username"?:U}`. RADIUS: `{"radiusToken":C,"skipSecondaryAuth":0\|1,"username"?:U}` | next-step fields |
| E18 | `POST /passport/v1/auth/challenge` | none | `{"radiusToken":RSA_HEX(C),"username"?:U}`. Exactly these members: with `skipSecondaryAuth` or any other addition the server answers code 75500000. | next-step fields |
| E19 | `GET /passport/v1/auth/accessCheck` | none | none | next-step fields |
| E20 | `GET /passport/v1/auth/{preEnhancedAuth\|enhancedConfirm\|enhancedDone}` | `authId` if known | none | next-step fields |
| E21 | `POST /passport/v1/auth/bindAuthDevice` | none | `{"reason":"","unbindList":"<JSON array text>"}` (unbindList is a **string** containing JSON, e.g. `"[]"` or `"[\"device-a\"]"`) | next-step fields plus `data.bindStatus`, `data.curBindNum`, `data.bindNumLimit`, `data.trustedDeviceList[]` = {`id`,`deviceName`,`deviceType`,`os`,`bindTime`} |
| E22 | `GET /passport/v1/user/onlineInfo` | none | none | `data.username` |
| E23 | `POST /controller/v1/user/clientResource` | none | `{"resourceType":{"appList":{},"favoriteAppList":{},"featureCenter":{},"sdpPolicy":{},"uemSpace":{"params":{"action":"login"}}}}` | entire body = policy document (§6). Code not checked here. |
| E24 | `GET /passport/v1/security/queryDevice` | `status=trust` | none (but Content-Type JSON header sent) | `data.selfId`, `data.deviceTrusted` (bool) |
| E25 | `POST /passport/v1/security/trustDevice` / `…/untrustDevice` | none | `{"idList":[selfId]}` | `code` |

Notes:
- `skipSecondaryAuth` is typed **inconsistently** across endpoints: an int in E17, the strings
  `"0"`/`"1"` in E14 and E16. Reproduce the types exactly.
- `isPrevEffect` is likewise inconsistent: the string `"0"` (E13 query, E15) and the bool
  `false` (E14 B, E16, E17).
- E3 request body: these are **exact bytes, including whitespace** (LF line breaks, 12/16/12/8
  space indents):
  ```
  "\n            {\n                \"nonce\": \"<NONCE>\",\n                \"ticket\": \"<TICKET>\"\n            }\n        "
  ```
  The E3 `x-csrf-token` is the CSRF token **from the authConfig response being validated**,
  not the previous one.
- HTTP status: E1, E2 and E3 must answer **200**. For E2 a 401 or 403 means the session is
  invalid, and any other status than 200 is a plain failure, at login as well as on refresh.
  An E3 reply with another status fails the second-stage check (§3.6). On E5, E10, E11, E22 and
  E23 the status is not looked at; only the body counts.
- On E2, E5, E10, E11 and E22 a body that is not valid JSON fails the step. E23 is the
  exception (§5.2).

### 4.4 Error / status codes

| Code | Where | Meaning / handling |
|---|---|---|
| 0 | all | success |
| 10000004 | authConfig, anti-MITM, all tunnel auths | session invalid / not found. **Fatal**: a full re-login is required. The reference process exits. |
| 75500002 | authConfig, tunnel auths | session expired. Fatal, as above. |
| 10000008 | antiMITMRequest | the server detected MITM |
| 75500401 | `auth/sms?action=sendsms` | treated as success (**(inferred)** "code already sent / still valid") |
| 75500000 | `auth/challenge` | request rejected: stale `antiReplayRand` or an unexpected field |
| 73600007 | tunnel auth | a non-zero code known only from a reference fixture; meaning unknown |
| any other ≠0 | | step failure. Message in `message`. |

---

## 5. Authentication state machine (`sangfor-atrust-auth`)

### 5.1 Session state

The machine holds: base URL/host, RID, cookie jar, CSRF token, RSA pubkey (mod hex, exp
decimal), antiReplayRand, sign key K, ticket, nextService, the qualified username
(`user@domain` or `phone@domain`), device ID, MAC list, optional TOTP secret, and a
way to put questions to the user (below).

Three kinds of question to the user occur:

- **a one-time code**: for SMS, TOTP and RADIUS factors, and for choosing devices to unbind
  (§5.6). For the second factors marked "skip option" in §5.5 the user may, along with the code,
  ask that secondary authentication be skipped; that choice travels as `skipSecondaryAuth` (1 or 0, in
  the type the endpoint uses, §4.3). Its effect on the server is an **open question**.
- **a click captcha**: the user is shown an image and returns the points clicked, optionally
  with the size at which the image was displayed.
- **an external login**: the user is given a CAS or OAuth2 login URL and pastes back the
  callback URL the browser ended on.

(EasyConnect additionally needs a typed-text captcha question; aTrust does not.)

### 5.2 Top-level login

```mermaid
sequenceDiagram
  participant C as Client
  participant P as Portal (HTTPS)
  C->>P: GET /public/manifest            (E1; fall back to cached copy on failure)
  C->>P: GET authConfig needTicket=1     (E2)
  Note over C: update csrf, pubKey, antiReplayRand, K;<br/>run anti-MITM (§5.3), result is advisory
  alt data.isLogin == 1
    C->>P: GET onlineInfo (E22)  → done (cookies already valid)
  else
    Note over C: pick authServerInfoList entry with<br/>authType == method && loginDomain == domain (else error)
    C->>P: primary method (§5.4)  → ticket, nextService
    C->>P: POST reportEnv (E10)   (requires ticket)
    loop secondary chain, ≤ 8 steps, starting at nextService or "auth/authCheck"
      C->>P: step (§5.5)
    end
    C->>P: GET onlineInfo (E22) → username
  end
  C->>P: POST clientResource (E23) → policy
  Note over C: snapshot cookies → sid (must be non-empty)
```

Rules for these steps on the password path:

- **Stored cookies.** Cookies from the persistence blob (§11.3), if there are any, are put into
  the jar before E2. That is how `isLogin == 1` comes about. E1 is sent before they are loaded.
- **authConfig (E2) at login.** The reply must have status 200 (§4.3) and a JSON body. A `code`
  member that is present and non-zero fails the login: 10000004 and 75500002 are reported as an
  invalid session, any other value as a plain failure. Login has no retry and no "transient"
  class; those belong to refresh (§5.8). Unlike refresh, a reply with no `code` member is
  accepted, and a missing `isLogin` counts as 0. `isLogin` is tested before a login method is
  looked for.

  No member of `data` is required at this point. `csrfToken`, `pubKey`, `pubKeyExp` and
  `antiReplayRand` (all JSON strings) are stored as received, empty or absent included, and
  replace the values held before. What a missing or empty member then leads to:

  | Member | Effect on the password path |
  |---|---|
  | `pubKey` | E5 is not sent; the step fails locally. The same holds for text that is not hex. |
  | `pubKeyExp` | E5 is not sent; the step fails locally. The same holds for text that is not a decimal integer greater than zero. |
  | `antiReplayRand` | Not detected. The plaintext of §3.4 becomes `secret ‖ "_"`. What the server answers is an **open question** (§13); an implementation may fail locally instead. |
  | `csrfToken` (in both places) | Not detected. `x-csrf-token` goes out with an empty value. |
  | `authServerInfoList` | No entry can be selected, so the login fails (unless `isLogin == 1`). |

- **Method entry.** The entry used is the first in list order whose `authType` and
  `loginDomain` both equal the wanted values exactly (case-sensitive). For `auth/psw` nothing
  else of the entry is used.
- **Start of the secondary chain.** The primary reply is *not* put through the resolution of
  §5.5. Only its `data.nextService` string is read; a `nextServiceList` in it is ignored. The
  chain's first service is that string taken literally, or `auth/authCheck` when it is empty,
  and it has no authId. So after `auth/psw` with an empty `nextService` the chain opens with
  E11, and the E11 reply is the first one resolved by the §5.5 table. Because the string is
  taken literally, `auth/sms` here runs in mode A, `auth/token` fails as undispatchable, and
  `auth/sendSms` is not rewritten to `auth/sms` and fails as unsupported. The last looks like
  an accident of the reference; an implementation may apply that rewriting here too.
- **reportEnv (E10)** is mandatory. It is sent only with a non-empty ticket; an empty one fails
  the login before the request. A transport error, a body that is not JSON, or a non-zero `code`
  fails the login. `mac_addresses` is the list from the persistence blob if it has one, else the
  configured list, else one freshly generated address (§3.8), so on this path it is never
  empty.
- **onlineInfo (E22).** A transport error, a body that is not JSON, or a non-zero `code` fails
  the login. `data.username` is taken as it is. An empty or missing value is accepted: the login
  succeeds with an empty user name, which the tunnels then send as an empty string (§9.2).
- **clientResource (E23).** Only a transport error fails the request. Neither the status nor
  `code` is looked at, and the body goes to the policy parser (§6) unchanged. A body that is
  not JSON fails there. A JSON error envelope without the policy members parses as an empty
  policy (no rules, no node groups), which the reference notices only later, when it has no
  node to connect to. An implementation should fail on a non-zero `code` here **(inferred)**.
- **Session identifier.** It is read once, after the E23 response has been processed: the value
  of the cookie named `sid` among the cookies the jar would send to `https://<base host>/`.
  Values `sid` had earlier in the login are not used. An empty or missing value fails the setup
  as an invalid session. The tunnels take nothing else from the jar: this one string is the only
  cookie-derived input to §7, §8 and §9, and a refresh replaces it (§5.8). The other cookies
  matter only for later control-plane requests and for the persistence blob (§11.3).

### 5.3 Anti-MITM inside authConfig

This runs only when `antiMITMAttackData` is present (raw object non-empty). Order:

1. challenge check (§3.3), 2. `mitmSig` check (§3.3), 3. if `enable != 1`, stop: OK,
4. the response must have arrived over TLS, then run the certificate-identity check (§3.5),
5. if `antiMITMRequest == true`, stop: OK. Otherwise perform E3 and validate it (§3.6).

**Failure policy:** in the reference client a failed check does not stop the login. The result
is recorded in the log and authConfig counts as successful. The reference project reports that
the vendor's own clients do likewise: they show the result to the user and carry on. A
clean-room implementation should expose a strict mode that aborts on failure (dual-use:
compliant default plus opt-in enforcement). The sign key K is updated whatever the outcome.

### 5.4 Primary methods

The method is selected by `authType` and `loginDomain` from authConfig's list. The selection
rule when the user names no type: username+password → `auth/psw`; phone → `auth/smsCheckCode`.

**Graph-captcha rule** (applies to E5, E6 and E7). The request is first sent with no captcha
answer. A reply carrying `data.graphCheckCodeEnable == 1` means the server wants a click captcha
solved before it will act on the request; `0` means none is outstanding. For as long as the
value is 1:

- fetch a fresh image (E4), then refresh authConfig (E2 with `needTicket=1`). The CSRF token,
  RSA key and `antiReplayRand` from that refresh are the ones the resend must use;
- have the user solve the captcha and send the same request again with `graphCheckCode` set to
  the answer described below.

The reference client fails the login after 5 captcha rounds; an implementation may choose its
own limit.

Answer format: compact JSON `{"coordinates":[[x,y],…],"width":w,"height":h}`, carried in
`graphCheckCode` as a JSON **string** whose content is that JSON. There must be at least one
point, each point is exactly two integers, and `w` and `h` are positive. They give the size of
the image the points refer to; when the user interface does not supply both, use the pixel size
read from the image header (§5.7).

**PSW (`auth/psw`)**, using E5 with `username = user@domain` and password encrypted per §3.4:
- `code != 0`: if `graphCheckCodeEnable == 1` → the captcha rule above applies. Otherwise the
  login fails with `code: message`.
- `code == 0`: the ticket must be non-empty unless `graphCheckCodeEnable == 1`. Store `ticket`,
  `nextService`, and the fresh `antiReplayRand` if one is present.
- Qualified username: `user ‖ "@" ‖ domain`, always. Nothing is checked or escaped. A user name
  that already contains `@` gets the suffix all the same (`a@b` with domain `local` is sent as
  `a@b@local`), and an empty domain gives `user@`.
- Login domain: the reference never derives it from `authServerInfoList`. It uses the
  configured string as it is, and an entry with exactly that `loginDomain` must exist (§5.2).
  When the user configures none, the reference's own configuration default applies, the literal
  `Radius`. That is a default of the project's home deployment, not a protocol value. Which
  entry the vendor client preselects is an **open question** (§13). An implementation should
  require the domain, and may offer the `loginDomain` values of the `auth/psw` entries to
  choose from.
- Of the reply's next-step fields only `data.nextService` is read (§5.2).

**SMS primary (`auth/smsCheckCode`)**: E6 (send) under the captcha rule, then ask the user for
the SMS code, then E7 (verify) under the captcha rule. E7 yields `ticket` as for PSW. Afterwards
the qualified username for later token steps is `phone@domain`.

**CAS (`auth/cas`)** and **OAuth2 (`auth/httpsOauth2`)**:
- Callback URL: built from a supplied CAS ticket (E8) or OAuth2 code (E9). Otherwise show
  the user the `loginUrl` from authConfig and have them paste back the callback URL. Validation:
  path must be exactly `/passport/v1/auth/cas` with a non-empty `ticket` query parameter, or
  `/passport/v1/auth/httpsOauth2` with a non-empty `code`.
- GET that URL with **redirects disabled**. Require status **302**.
- Parse `Location`: scheme `https`, host **equal to the base host** (including `:port`), path
  exactly `/portal/shortcut.html`, query `data` non-empty. `data` is JSON
  `{"ticket":"<portal ticket>"}` and the ticket must be non-empty.
- Then GET authConfig with `mod=1` and no `needTicket`.

### 5.5 Secondary chain: next-step resolution and dispatch

**Resolution.** A reply's next-step fields are `nextService` (a string, possibly empty) and
`nextServiceList[]`. First pick the list entry *e*: the one whose `authType` equals
`nextService`, or failing that the first entry. An empty list gives no entry. When an entry
exists, its `authId` is the step's authId. The step's service then follows from this table:

| `nextService` | Entry *e* | Service |
|---|---|---|
| empty | none, or one with neither `authType` nor `authId` | none: the chain is complete |
| empty | `authType` non-empty | `e.authType` |
| empty | `authType` empty, `authId` non-empty | `auth/sms` (the reply shape of older gateways) |
| `auth/token` | `authType` ∈ {`auth/totp`, `auth/radius`, `auth/challenge`} | `e.authType` |
| `auth/token` | `authType == "auth/token"` and `subType == "totp"` | `auth/totp` |
| `auth/token` | anything else, or none | `auth/token` (cannot be dispatched, see below) |
| any other value | any | `nextService` unchanged |

Wherever the result is `auth/sendSms` it is treated as `auth/sms`. An `auth/sms` step has two
variants: **mode B** when an authId is known, and **mode A** when it is not (the server then
keeps the SMS state in the session).

**Dispatch.** Each step's reply is resolved as above, and the chain continues until no service
remains. The reference client gives up after 8 steps; an implementation may choose its own
bound. (The reference counts the final "none" as one of the eight, so a chain succeeds there
only with seven steps or fewer.)

| service | Action |
|---|---|
| none | finished |
| `auth/authCheck` | E11 |
| `auth/sms` | Mode B only: refresh authConfig first (E2, `needTicket=1`). Both modes: E12 to tell the user which number is used (a failure here is tolerated), then E13 to send. Mode A only: refresh authConfig (E2, `needTicket=1`) once the SMS is sent. Then ask the user for the code (skip option) and submit it with E14. |
| `auth/customSms` | E15, ask the user for the code (skip option), E16 |
| `auth/totp` | The code is computed from the configured TOTP secret when there is one; otherwise ask the user (skip option). Submit with E17, TOTP form. |
| `auth/radius` | Ask the user for the code (skip option). Submit with E17, RADIUS form, token in clear. |
| `auth/challenge` | Ask the user for the code. Submit with E18, token RSA-encrypted per §3.4. |
| `auth/accessCheck` | E19 |
| `auth/preEnhancedAuth`, `auth/enhancedConfirm`, `auth/enhancedDone` | E20 for that path |
| `auth/bindAuthDevice` | §5.6 |
| `auth/token` | failure: the server did not say which token type applies |
| anything else | failure: step not supported |

The `username` field in E17/E18 is included only if a qualified username is known (psw or SMS
primary). With CAS/OAuth2 the session identifies the user and the field is omitted.

### 5.6 Device binding (`auth/bindAuthDevice`)

1. E21 with `unbindList:"[]"`.
2. If `bindStatus != 1`, resolve the next step from the response.
3. Otherwise the device quota is full, and `trustedDeviceList` must be non-empty. The number of
   devices to remove is `curBindNum − bindNumLimit + 1`, and never fewer than one. Ask the user
   which of the listed devices to remove (the reference shows a numbered list and takes that
   many comma-separated numbers; the presentation is the implementation's choice). Send E21
   again with `unbindList` = the JSON array text of the chosen devices' `id` values, and resolve
   the next step from that response.

### 5.7 Image size sniff (captcha fallback)

PNG: IHDR width/height (u32 BE at offsets 16 and 20). GIF: logical screen width/height (u16
**LE** at offsets 6 and 8). JPEG: first SOFn marker, height then width (u16 BE). Reject zero
sizes.

### 5.8 Session refresh (maintenance)

- Periodically (reference default **1800 s**, configurable, 0 disables): E2 with `needTicket=0`
  (no `mod`), using the current cookies and CSRF.
- **Session invalid** (fatal, re-login needed): HTTP 401/403; code 10000004 or 75500002;
  `isLogin != 1`; or no `sid` cookie after the response (deleted via `Max-Age<0`).
- **Transient** (keep the old sid and try again; the reference waits the shorter of the refresh
  period and 60 s): other HTTP status, other non-zero code, missing `code`, missing `isLogin`,
  malformed JSON, network error.
- On success: update CSRF, K and the other authConfig state. Read the new `sid` and hand it to
  the tunnels, so new tunnel auth requests use the latest sid. Persist the cookies.
- A refresh only extends a session that is still valid. On an invalid session it reports that
  and stops; whether to log in again is the caller's decision. It must be cancellable.

### 5.9 Trusted-device management (optional utility)

Log in, or reuse cookies with no login method. Then E24. For **trust**: if
`deviceTrusted == false`, E25 trust with `[selfId]`. For **untrust**: the inverse.

---

## 6. Policy model (`sangfor-atrust-policy`)

Source: `client/atrust/parse.go`, tests `parse_resource_test.go`, `node_test.go`;
`internal/ipresource/index.go`; `client/client.go` **[shared]**.

### 6.1 Document paths (keys matched case-insensitively)

| Path | JSON type | Meaning |
|---|---|---|
| `data.appList.data.appInfo[].apps[]` | array of objects | applications |
| … `.id` | string | application id (`appId` in tunnel auth) |
| … `.nodeGroupId` | string | node group that serves the application |
| … `.accessModel` | string | only `L3VPN` is used (§6.2) |
| … `.enableTCPPrefL3` | boolean | TCP to this application prefers the L3 tunnel (§6.5) |
| … `.addrPretend` | any JSON value | see §6.2 |
| … `.addressList[]` | array of objects | members `protocol`, `port`, `host` (strings) and `ip` (array of strings) |
| `data.appList.data.config.nodeGroupConf.majorNodeGroup.id` | string | default node group |
| `data.appList.data.config.nodeGroupConf.nodeGroupList[]` | array of objects | members `id` (string) and `addressInfo[]`, each with `address` and `type` (strings) |
| `data.sdpPolicy.data.clientOption.dnsOption` | object | members `firstDNS`, `secondDNS` (strings) |
| `data.sdpPolicy.data.clientOption.dnsOptionV2` | object | same members |
| `data.sdpPolicy.data.clientOption.tun0rtt` | object | members `enable`, `preConnNum`, `minIdleConnNum`, `maxIdleConnNum`, `maxIdleLingerTime`; read by the reference but **not** acted on |

Key spelling: the reference matches all of these keys without regard to letter case. The lower
camel case shown here is this document's convention, not a confirmed server spelling, so
`id`/`ID`, `nodeGroupId`/`NodeGroupID` and the like must all be accepted. The exact server
casing is an **open question**.

### 6.2 Application → rules

Only apps with `accessModel == "L3VPN"` are used. Others (e.g. `WEB`) are ignored.

For each address with `protocol` ∈ {`tcp`,`udp`,`all`} (others skipped, so ICMP needs `all`):

- **Port**: `"a-b"` → [a,b]. `"n"` → [n,n]. Malformed → skip the address.
- **Host classification**, in order:
  1. IPv4 literal → an IP rule [ip, ip]. (IPv6 literals are skipped.)
  2. IPv4 CIDR → an IP rule [network, broadcast]. (IPv6 CIDR is skipped.)
  3. `ipA-ipB` with both parsing as IPs → an IPv4 range rule. (IPv6 is skipped. Note that
     `"a-b"` where a or b is not an IP becomes a domain.)
  4. otherwise **domain**: trim. A `*.example.org` pattern becomes the suffix `.example.org`.
     Any other `*` means the address is dropped. The result is a domain rule
     {ports, protocol, appId, nodeGroupId, enableTCPPrefL3, addrPretend}.
- **addrPretend**: `true` if the JSON value is boolean `true` **or absent / non-boolean**.
  `false` only for boolean `false`.
- **Pinned IPs** (`ip[]`): only for domain hosts. Each IPv4 entry adds a DNS pin
  `host → ip` (the raw host string, not the normalised one) and an IP rule [ip, ip] with the
  same app metadata. IPs listed on a non-domain host are ignored.
- Rule **order** = document order. It matters for first/last matching.

Details the list above leaves open:

- **Value spelling.** `accessModel` and `protocol` are compared exactly, so only `L3VPN`, `tcp`,
  `udp` and `all` in this letter case count: an app with `l3vpn` is ignored and an address with
  `TCP` is skipped. Whether servers ever send another case is an **open question** (§13).
- **Missing ids.** An app with an empty or missing `nodeGroupId` yields its rules as usual, and
  their node comes from the major group (§6.6 step 6). An app with an empty or missing `id`
  also yields its rules, with an empty appId. The TCP tunnel treats an empty appId as "no rule"
  (§6.6 step 5). An implementation may drop such an app while parsing **(inferred:** a tunnel
  request needs an appId).
- **Port strings.** The text is used as received, without trimming. Without a `-` it must be one
  decimal integer; with one it must split into exactly two. Everything else is malformed and
  skips the address: the empty string, white space anywhere, `80-`, `-80`, `1-2-3`. A skipped
  address yields nothing at all: no rule, and no pin from its `ip[]`. (The reference's integer
  parser also lets a leading `+` and leading zeros through, so `+80` and `080` are 80. That is
  an accident, and an implementation may reject them.)

  The numbers are not range-checked. Zero, values above 65535 and a range with a > b are all
  kept, and their effect follows from the test in §6.5: a range with a > b contains no port, an
  upper bound above 65535 acts like 65535, a lower bound above 65535 contains no port, and `0`
  is an ordinary bound, so `0-1023` includes port 0. A rule whose range contains no port can
  still match ICMP when its protocol is `all`, because ICMP ignores ports. An implementation may
  clamp the bounds to 0–65535. If it drops rules with an empty range instead of keeping them, it
  differs from the reference in that ICMP case only.
- **Host strings.** Steps 1–3 look at the text exactly as received; only step 4 trims.
  - An IPv4 literal is strict dotted decimal: four numbers 0–255 without leading zeros. The
    reference also takes an IPv4-mapped IPv6 literal (`::ffff:a.b.c.d`) for that IPv4 address.
  - A host with white space around it is therefore never an address, a CIDR or a range. It
    reaches step 4 and becomes a *domain* rule named by the trimmed text (` 192.0.2.7` gives a
    domain rule named `192.0.2.7`). No lookup can select such a rule, since a destination
    written as an address is never looked up by name (§6.6). This is an accident of the
    reference; an implementation may trim before step 1 **(inferred** to be the intent).
  - CIDR `a.b.c.d/n`, n from 0 to 32: the rule runs from the network address (host bits
    cleared) to the broadcast address. `/32` is the single address, `/0` is 0.0.0.0 to
    255.255.255.255, and `198.51.100.77/24` gives the same rule as `198.51.100.0/24`.
  - `ipA-ipB` needs exactly one `-`, and A decides the address family. The bounds are not
    reordered. A rule with A > B, or with an IPv4 A and an IPv6 B, is created and keeps its
    place in the order, but it matches nothing.
- **Where pinned addresses sit in the order.** IP rules form one sequence for the whole
  document. Domain rules are kept per name (§6.5) and have no place in that sequence. Each
  `addressList` entry appends to the IP sequence either the one rule for its own host (steps
  1–3) or, for a domain host, one rule per IPv4 entry of `ip[]` in array order. Pinned-address
  rules thus follow every IP rule of earlier entries (earlier apps included), precede those of
  later entries, and keep array order among themselves. Entries of `ip[]` that are not IPv4
  addresses are skipped one by one. The pins of one host accumulate across entries in the same
  order, duplicates included. A host dropped for its `*` (step 4) yields neither pins nor IP
  rules. A wildcard host `*.example.org` with `ip[]` does yield the IP rules, but its pin is
  filed under the text `*.example.org`, which no queried name can equal.

### 6.3 DNS servers

Use `dnsOption.firstDNS` and `secondDNS`. If `dnsOption.firstDNS` is empty, use
`dnsOptionV2` instead. Empty entries are dropped. The primary is the first.

### 6.4 Node groups

For each `nodeGroupList[]` entry, each `addressInfo` maps as follows. The literal
`{{sdpcHost}}` is replaced by the configured server address. If the address contains no `:`,
`:441` is appended (**open question**: IPv6 literals). `type` (case-insensitive) is `wan` or
`lan`; anything else is ignored. A node whose address is an IPv4 literal must itself stay
outside the tunnel: take that address out of the destinations the policy would otherwise
capture, or the tunnel's own traffic would be sent into the tunnel. `majorNodeGroup.id` is the default group.

The `{{sdpcHost}}` replacement happens only when the whole `address` is exactly that text. The
value put in its place is the configured server host without a port, so `:441` is then added.

The exact effect of keeping node addresses out: the policy yields two separate things. One is
the rule lists of §6.2, which every lookup uses. The other is a set of IPv4 addresses, the
union of all IP-rule ranges and pinned addresses, which serves only to install operating-system
routes when the client runs a TUN device. Only that set is reduced. Each node address that is an
IPv4 literal is taken out of it as a single address. This is done for every `addressInfo` entry,
those with an ignored `type` included, and for the configured server address when it stands in
for `{{sdpcHost}}`. Node addresses given as names are not resolved for this. The rule lists stay
as they are: an IP lookup (§6.5) for a node's address returns whatever rule covers it, exactly
as for any other address. A client that installs no routes therefore has nothing to take out.
If it wants the same protection it can decline to tunnel a destination that is one of its own
node addresses **(inferred)**.

### 6.5 Matching semantics **[shared with EasyConnect]**

IP rule = {IPv4 min, max, port min, max, protocol, appId, nodeGroupId, enableTCPPrefL3}.
Lookup(ip, proto, port) considers rules where:
- the IP is within [min, max] (IPv4 only),
- `rule.protocol == proto` or `rule.protocol == "all"`,
- `proto == "icmp"` (ports ignored) or the port is within [min, max],
- an optional predicate holds.

Two policies apply. **First** match takes the lowest document order; **Last** match takes the
highest. An interval tree with max-end augmentation gives O(log n + k) lookups, but any
correct structure will do.

Domain lookup(domain, proto, port): the first rule in that domain's list with the same
protocol/port test (and an optional predicate).

"That domain's list" is the *candidate list* of the queried name, built as follows.

- **Normalisation.** Rule names and queried names are compared after the same treatment:
  surrounding white space removed, letters lower-cased, then one trailing dot removed. For a
  rule this comes on top of §6.2 step 4, so the policy host `*.EXAMPLE.Net.` is the suffix
  `.example.net`. A rule whose name is empty after this is ignored. Comparison is bytewise, and
  no IDNA conversion is applied.
- **Selection.** A rule name R selects a queried name Q when R equals Q, or when Q ends with R
  and either R begins with a dot or the character of Q just before that ending is a dot. A plain
  name `example.org` therefore covers itself **and every name beneath it** (`www.example.org`,
  but not `badexample.org`). A wildcard-derived suffix `.example.org` covers the names beneath
  it and does **not** cover the bare `example.org`.
- **Order.** The candidate list holds the rules of every selecting name, the longest name's
  rules first. An exact name is always the longest, so it precedes any suffix, and a more
  specific suffix precedes a shorter one. Where a plain name and a suffix differ only in the
  leading dot, the suffix is the longer and comes first. Rules that share a name stay in
  document order.
- **Result.** The first candidate that passes the protocol/port test (and the predicate) is
  returned. A longer name whose rules all fail the test does not hide the shorter names' rules.

Policy hosts that differ in spelling but normalise to one name share that name's rules. The
reference leaves the order between such spellings to chance; use document order.

Routing choice per destination (consumer policy, **[shared]**):
- **L3 tunnel** accepts UDP/ICMP on any matching rule (first match). It accepts TCP **only**
  if the first matching rule satisfying the predicate has `enableTCPPrefL3 = true`.
- **TCP tunnel** uses the **last** matching TCP rule with `enableTCPPrefL3 = false`. With the
  caller's *take-all* flag (§6.6) it falls back to the last matching TCP rule of any kind.
  A domain rule with `enableTCPPrefL3` is refused for the TCP tunnel unless that flag is set.

### 6.6 From a destination to a TCP tunnel request

This is how the reference decides, for one outbound TCP connection to `host:port`, whether it
goes through the TCP tunnel (§9) and with which values. Steps 1–5 are consumer policy
**[shared]**; the gateway sees only their result (§9.2). "Not covered" means the tunnel is
not used. The reference then connects directly, outside the VPN, or refuses the connection
when it is configured to send everything through the VPN.

The caller's *take-all* flag (§6.5) is set when the client has no L3 tunnel to give TCP to, as
in an implementation of the TCP tunnel alone.

1. **IPv6 literal**: not covered. No lookup is made. (IP rules are IPv4 only, and the
   reference never produces the ATYP `04` form of §9.1.)
2. **IPv4 literal**: there is no name. Continue at step 5.2.
3. **Host name**: normalise it and collect its candidate list (§6.5), which may be empty. The
   normalised name is "the name" from here on.
4. **Resolve the name to an address.** The reference always does this, even when the rule
   chosen in step 5 has addrPretend = true and the address then goes unused, because its
   internal interfaces work on addresses. The sources, in order; the first answer wins:
   1. The client's own table: host entries configured by the user, and answers remembered from
      source 3.
   2. The policy's DNS pins (§6.2) for the name. The name must equal the pin's host text byte
      for byte, and that text is the *raw* policy string. A pin whose host has upper-case
      letters, a trailing dot or surrounding white space is therefore never found. This is an
      accident; an implementation may normalise the pin's host too **(inferred)**. With several
      addresses the reference rotates: the first lookup of a name gets the first address, the
      next lookup the second, and so on round.
   3. The policy's primary DNS server (§6.3), asked for A records only, **through the VPN**, on
      port 53. Over UDP that needs the L3 tunnel. Over TCP it is an ordinary TCP tunnel stream
      to `<DNS server>:53`, chosen by step 5.2 like any other address, so an IP rule has to
      cover the DNS server. The reference tries UDP first and starts TCP as well once UDP has
      failed or has gone 300 ms without an answer; a client without L3 ends up on TCP alone.
      The first address of the first answer is used, and remembered for a configurable time
      (reference default 3600 s). This source is skipped when the policy names no DNS server.
   4. A resolver **outside the VPN**, on the host's own network: the policy's second DNS server
      when there is one, else a server of the client's choosing. It is asked for A records, and
      for AAAA when that fails. (The reference really does query the policy's second server
      from outside the tunnel, which works only where that server is reachable there.)

   No answer from any source: not covered. An IPv6 answer: not covered.

   When the chosen rule has addrPretend = true the gateway resolves the name itself (§9.2), so
   an implementation may skip this step for such a rule **(inferred)**. The reference would
   instead have given up on a name it cannot resolve itself.
5. **Choose the rule**, for protocol `tcp` and the destination port:
   1. *By name.* From the candidate list take the first rule that passes the protocol/port test
      of §6.5 and has enableTCPPrefL3 = false; failing that, the first that passes the test at
      all. If the rule taken has enableTCPPrefL3 = true and take-all is not set, the connection
      is not for the TCP tunnel (it belongs to L3). Otherwise it is the chosen rule, a *domain
      rule*, unless its appId is empty.
   2. *By address.* This applies when there is no name, when step 5.1 found no rule, and when
      the rule it found has an empty appId. Do an IP lookup (§6.5) on the address: the **last**
      matching rule with enableTCPPrefL3 = false; failing that, and only with take-all set, the
      last matching rule of any kind. A rule found here is an *IP rule*, and a name, if there
      was one, plays no further part. A found rule with an empty appId counts as no rule, even
      when an earlier rule that has an appId also matches (an accident of the reference).
   3. No rule: not covered. (Without take-all, an address all of whose matching rules have
      enableTCPPrefL3 = true is not lost: it belongs to L3.)
6. **Choose the node.** Take the best node (§10) recorded for the rule's node group; if none is
   recorded, the one recorded for the major group; if there is none either, the connection
   fails without any I/O. A best node is recorded only for a group in which a probe succeeded,
   and the reference never falls back to an address it has not probed. An empty or unknown
   `nodeGroupId` has no record, so it leads to the major group. An implementation that does not
   probe needs a choice of its own here, for example the group's first WAN address, else its
   first LAN address, else the same for the major group **(inferred)**.
7. **Fill in the request** from the kind of rule, the name and the address: §9.2.

---

## 7. Tunnel session authentication (`sangfor-atrust-wire`, used by VIP fetch and L3)

Source: `client/atrust/l3tunnelconn.go`, `client/atrust/ip.go`, test `ip_test.go`,
`l3tunnel_lifecycle_test.go`. Transport: TLS (§2.1) to a node `host:port`.

### 7.1 Client → server (one write)

```
 0      1      2      3      4      5      6      7 …            7+N
+------+------+------+------+------+------+------+-- … ---------+
| 0x05 | 0x01 | 0xD0 | 0x53 | 0x00 |  N (u16 BE) | JSON (N bytes) |   greeting + "S" sub-negotiation
+------+------+------+------+------+------+------+-- … ---------+
| 0x05 | 0x04 | 0x00 | 0x01 | 0x00 0x00 0x00 0x00 | 0x00 0x00 |      request: VER, CMD=4, RSV, ATYP=1, 0.0.0.0, port 0
+------+------+------+------+---------------------+-----------+
JSON = {"sid":"<current sid>"}
```

The greeting is VER=5, NMETHODS=1, METHOD=0xD0 (Sangfor private method). The sub-negotiation
header is 0x53 (`'S'`) and status 0x00. The request's CMD **0x04** ("query VIP",
**(inferred)** meaning) and ATYP byte are fixed at 1 in every observed use.

### 7.2 Server → client

```
+------+------+                                    method reply
| 0x05 | 0xD0 |
+------+------+------+------+---- … ----+         auth reply
| 0x53 |  ST  | L (u16 BE)  |  JSON (L)  |
+------+------+------+------+---- … ----+
| 0x05 |  VS  |  XX  |  AT  |  VIP data (len by AT)  |   VIP reply
+------+------+------+------+-----------------------+
```

- Method reply must be exactly `05 D0`, else fail.
- Auth reply: the first byte must be `0x53`. **Always read all L bytes** before acting on ST,
  to keep framing. If L > 0, parse JSON `{"code","message","data":{"deviceId"}}`: code
  10000004/75500002 is fatal; any other non-zero code fails. ST ≠ 0 fails. Success is decided by ST and `code` alone, never by the `message` text.
- VIP reply header: byte0 must be `0x05`, VS must be `0x00`, and **XX is ignored** (observed
  `0x00`, `0x01`, `0x7F`). AT selects the data length:

| AT | Data length | Layout **(trailing 2 bytes inferred to be a port, ignored)** |
|---|---|---|
| 0x01 | 6 | IPv4[4] ‖ 2 bytes |
| 0x04 | 18 | IPv6[16] ‖ 2 bytes |
| 0x05 | 22 | IPv4[4] ‖ IPv6[16] ‖ 2 bytes |
| other | — | error |

The **VIP fetch** connection (setup only) requires AT = 0x01 and uses the IPv4. It has a 10 s
overall timeout and a 10 s read deadline, and the connection is closed afterwards. On an **L3
tunnel** connection all three forms are accepted, and the VIP(s) are passed to the VIP-update
hook (§8.6). The connection then continues with L3 framing (§8).

---

## 8. L3 tunnel (`sangfor-atrust-wire` + `sangfor-atrust-l3`)

### 8.1 Frame formats (after §7 completes on the same TLS stream)

Version byte `V = 0x05`.

**Client → server**

| Cmd | Name | Layout |
|---|---|---|
| 0x13 | flow auth request | `05 13 ‖ len:u16 ‖ JSON(len)`, with **no status byte** |
| 0x14 | data | `05 14 ‖ tl:u8 ‖ token[tl] ‖ 00 00 ‖ count:u8 (=1) ‖ len:u16 ‖ packet[len]` |
| 0x15 | heartbeat | `05 15 00 00` |

```
data request (0x14)
+----+----+----+----------------+----+----+-------+-----------+----------------+
| 05 | 14 | tl | connect token  | 00 | 00 | count | len u16   | IP packet      |
+----+----+----+----------------+----+----+-------+-----------+----------------+
                 tl ≤ 255 bytes   reserved   =0x01   BE          one whole packet
```

The connect token is copied byte-exact (spaces preserved). An empty token gives `tl = 0`. The
client always sends `count = 1`. The count field implies the server may accept more than one
`len‖packet` pair per frame **(inferred)**. A decoder for this format should accept count ≥ 1.

**Server → client**

| Cmd | Name | Layout |
|---|---|---|
| 0x93 | flow auth response | `05 93 ‖ status:u8 ‖ len:u16 ‖ JSON(len)` |
| 0x96 | VIP update | `05 96 ‖ status:u8 ‖ len:u16 ‖ JSON(len)` |
| 0x94 | data | `05 94 ‖ len:u16 ‖ bytes(len)`. The bytes are a **stream of concatenated raw IP packets**, which may split across frames. |
| 0x95 | heartbeat response | `05 95 ‖ len:u16 ‖ bytes(len)` (contents ignored; normally len = 0) |
| other / version ≠ 5 | protocol error → close the connection | |

Note the asymmetry: upstream data carries token + count + per-packet length. Downstream data is
raw IP bytes with no per-packet header.

**Downstream re-assembly**: append each 0x94 payload to a per-connection buffer, then
repeatedly extract packets:
- version nibble 4: need ≥ 4 bytes. IHL = (b0 & 0x0F)·4, total = u16 at offset 2. **Error**
  (close) if IHL < 20 or total < IHL. Wait for `total` bytes.
- version nibble 6: need ≥ 6 bytes. total = 40 + u16 at offset 4.
- anything else is an error (close).

### 8.2 Per-flow authorisation (0x13 / 0x93)

Every distinct flow must be authorised before its packets are sent.

**Flow identity**: a flow is the address family plus the 5-tuple (IP protocol, source address,
source port, destination address, destination port). ICMP uses port 0 on both sides. How flows
are keyed locally is the implementation's choice. Only TCP (6), UDP (17) and ICMP (1) packets
are forwarded; a packet of any other protocol matches no resource and is not sent.

**Auth request JSON**: fields in this order (compact). Signed per §8.4.

| # | Key | Value |
|---|---|---|
| 1 | `sid` | current session sid (latest after refresh) |
| 2 | `appId` | appId of the matched rule |
| 3 | `procHash` | same as `env…fingerprint` (omitted if empty) |
| — | `appToken` | omitted (always empty) |
| 4 | `url` | `<proto>:<destination address>:<destination port>`, proto ∈ `tcp`,`udp`,`icmp`,`icmp6`,`ip` (note: **no** `//`) |
| 5 | `deviceId` | device ID |
| 6 | `connectionId` | connection ID |
| 7 | `rcAppliedInfo` | `0` (always present) |
| 8 | `lang` | locale `xx-YY` (from env, default `en-US`) |
| 9 | `env` | object, §8.3 |
| 10 | `conntrackHash` | u64. A number the client picks to identify this flow's request; the server echoes it. The reference counts up from 1 per node group. |
| 11 | `ip` | `{"atype":2048,"protocol":P,"destAddr":"a.b.c.d","destPort":n,"srcAddr":"…","srcPort":n}`. atype is the EtherType: 2048 (0x0800) for IPv4, 34525 (0x86DD) for IPv6. |
| — | `domain` | omitted (always empty) |
| 12 | `xRequestSig` | §8.4 |

**Auth response JSON**: `{"code","message","data":{"vipType"?,"vip6Type"?,"vip4Type"?,"conntrackHash":u64,"connectToken"?:string,"ip":{…same shape…}}}`.

Handling, in order:
1. JSON code 10000004 or 75500002 → **fatal session invalid**, before any other processing.
2. Find the flow by `data.conntrackHash`. Fall back to matching `data.ip` (atype, srcAddr,
   srcPort, destAddr, destPort) against pending flows. If no flow matches → drop the response
   (if JSON parsing failed and a conntrackHash is present, fail that flow).
3. Outer `status`:
   - `0x00` → success. **The JSON `code` is otherwise ignored.** `connectToken` (possibly empty)
     is stored for the flow. Flush the cached packets in order as 0x14 frames with that token.
   - `0x84` → retry **immediately**. This does not consume the timeout budget.
   - `0x85`–`0x87` → server busy: retry after **10 s**. This does not consume the timeout budget.
   - other → the flow fails. It is removed, pending packets are dropped, and the tunnel stays up.

Only the outer status byte is used for retry decisions, never the JSON code or the message.

### 8.3 `env` object (L3 and TCP tunnel)

```
{"application":{"runtime":{
   "process":{"name":N,"digital_signature":"TrustAppClosed","platform":PL,
              "fingerprint":HEX(SHA256(PATH)),"description":"TrustAppClosed",
              "path":PATH,"version":"TrustAppClosed","security_env":"normal"},
   "process_trusted":"TRUSTED"}}}
```

L3 uses N = `zju-connect`, PATH = `/usr/bin/zju-connect`, and PL = the OS name title-cased
(`Linux`, `Windows`; `Darwin` → `macOS`). The server presumably does not check process
identity strictly **(inferred)**. An implementation may choose its own name and path; those
values are not protocol constants.

### 8.4 `xRequestSig` (L3 and TCP tunnel)

1. Serialise the request with `"xRequestSig":""` (an empty string, the key still present in
   its slot).
2. Compute `HEX(HMAC-SHA256(key, those bytes))`, where the key is the tunnel sign key of §3.8
   after hex-decoding.
3. Serialise again with that value in place of the empty string, and send the result.

### 8.5 Flow table and timers (client policy)

What the protocol requires is short: a flow is authorised (§8.2) before its first packet is
sent, the `connectToken` it was given accompanies every later packet of that flow, and a flow
the client has forgotten is authorised afresh. Everything else in this section is the
reference client's own policy. An implementation may choose different values and a different
structure; the vendor client's timers are an **open question**.

*Per outbound packet.* The packet must match a rule (§6.5). Packets of an authorised flow go
out at once in a 0x14 frame with that flow's token. Packets of a flow still waiting for its
authorisation are held and then released in arrival order. Packets of a flow whose
authorisation failed are refused. Order within a flow must be preserved, and a flow that is
waiting must not hold up other flows.

*Authorisation scheduling in the reference.* Up to 64 waiting flows are requested per pass,
longest-idle first. A pass runs every 250 ms and whenever a new flow appears. A request with no
answer after 5 s is sent again, and the flow is failed once three requests have gone
unanswered. Status 0x85–0x87 postpones the resend by 10 s (§8.2). Up to 1024 packets are held
per waiting flow, with any excess discarded silently. Up to 16384 flows are tracked per node
group; beyond that the least recently used is discarded.

*Idle lifetimes in the reference.* A flow unused for longer than its lifetime is forgotten.
Whenever a flow is forgotten or discarded, a request still outstanding for it is failed.

| Flow | Idle lifetime |
|---|---|
| UDP | 120 s |
| ICMP | 30 s |
| TCP, handshake under way | 60 s after a SYN sent by the client or after a SYN-ACK; 120 s after a SYN that arrived from the tunnel |
| TCP, established | 6 h |
| TCP, both directions closed | 30 s when the client sent its FIN first; 120 s when the peer's FIN came first |
| TCP after a RST | 90 s |
| anything else | 60 s |

A TCP flow counts as established when the client's reply acknowledges the peer's sequence
number plus one: its ACK of a SYN-ACK for a connection the client opened, or its SYN-ACK for a
connection opened from the tunnel side. A single FIN leaves the lifetime as it was. Packets arriving
from the tunnel belong to the flow with source and destination exchanged. The reference removes
expired flows whenever a new flow is created and on each heartbeat period.

### 8.6 Keepalive, VIP updates, connection lifecycle

**Heartbeat.** The client sends `05 15 00 00` and the server answers with a 0x95 frame. When to
send is client policy. The reference uses a 5 s period that starts when the connection is up,
so no heartbeat goes out during the first period. At the end of a period it sends a heartbeat
only if it wrote no frame at all during that period. A 0x95 reply marks the peer as alive. The
stated intent is to close the connection after 3 heartbeats in a row go unanswered. However,
the reference counts its own heartbeat as a frame written until a reply arrives. An unanswered
heartbeat is therefore followed by a period with no heartbeat, the unanswered count starts
again, and the limit is never reached. In practice a dead peer is noticed only through read or
write errors (§13). Whether the server drops a connection that never sends heartbeats is an
**open question**.

**VIP update (0x96)**: ignored if status ≠ 0. Otherwise parse the JSON for `vip`/`vip6` at
top level, falling back to `data.vip`/`data.vip6` (`vip_type`/`vip6_type` are ignored). Accept
`vip` only if it is IPv4 and `vip6` only if it is not IPv4. If the IPv4 VIP changed: apply it
to the local stack (if that fails, keep the old VIP), update the client IP, and exclude it from
the underlay. Initial VIP data from §7 goes through the same hook.

**One connection per node group.** The connection for a group goes to the best node of that
group (§10), or failing that to the best node of the major group; with neither there is no
route. Each connection attempt is a TLS dial followed by the §7 session auth. The flows' connect
tokens belong to the **group, not the connection**: after a reconnect, flows that were already
authorised keep sending with the token they have.

The rest is the reference client's policy, which an implementation may replace:

- Connections are opened on demand. Simultaneous requests for one group share a single attempt.
  An attempt is given **10 s**. After a failure the reference tries again every **5 s** without
  limit, until it succeeds or the tunnel is closed.
- A read error discards the connection, and a new one is opened in the background. A write error
  that shows the connection is closed does the same, and the packet is retried once on the new
  connection or else dropped. Packets that arrive while a reconnect is pending are dropped, not
  queued.
- Downstream queues hold 128 packets per connection and 4096 for the tunnel as a whole. When
  one is full, packets are dropped while control frames continue to be processed.
- After a node re-probe (§10), a group whose best node changed has its connection closed. It is
  reopened on the next packet.

```mermaid
sequenceDiagram
  participant S as Local stack
  participant T as L3 engine
  participant N as Node (TLS :441)
  T->>N: TLS handshake (no verify, no tickets)
  T->>N: 05 01 D0 | 53 00 len {"sid":…} | 05 04 00 01 00×6
  N-->>T: 05 D0 | 53 00 len {code:0…} | 05 00 xx AT vip…
  S->>T: IPv4 packet (new flow)
  T->>T: cache packet, schedule auth
  T->>N: 05 13 len {sid,appId,…,conntrackHash:1,…,xRequestSig}
  N-->>T: 05 93 00 len {data:{conntrackHash:1,connectToken:"…"}}
  T->>N: 05 14 tl token 00 00 01 len packet   (flush cached, in order)
  N-->>T: 05 94 len <IP packets…>
  T->>S: packets
  Note over T,N: idle 5 s → 05 15 00 00 ; ← 05 95 00 00
  N-->>T: 05 96 00 len {"vip":"…"}  (optional VIP change)
```

---

## 9. TCP tunnel (`sangfor-atrust-wire` + `sangfor-atrust-tcp`)

Source: `client/atrust/tcptunnel.go`, test `tcptunnel_test.go`. One fresh TLS connection per
proxied TCP stream. Target node = best node of the rule's group, else the major group.
Handshake read deadline = **18 s**, or the caller's earlier deadline.

### 9.1 Handshake bytes

**Client → server** (one write: init ‖ destination):

```
init:
+----+----+----+----+----+-------------+---------------- … --+
| 05 | 01 | 81 | 53 | 03 |  N (u16 BE) |  signed JSON (N)      |   N ≤ 0xFFFF else error
+----+----+----+----+----+-------------+---------------- … --+
destination (SOCKS5 CONNECT shape):
+----+----+-----+------+-------------------------+----------+
| 05 | 01 | RSV | ATYP | address                 | port u16 |
+----+----+-----+------+-------------------------+----------+
RSV  = 0x01 if zero-RTT is enabled (§11.1), else 0x00
ATYP = 0x01 IPv4[4] | 0x04 IPv6[16] | 0x03 len:u8 ‖ name   (len byte is truncated mod 256; the name is written in full)
```

The method byte is **0x81** here (versus 0xD0 for session auth). The sub-negotiation status
byte is **0x03** (versus 0x00).

**Server → client**:

```
+----+----+                         server hello: must be exactly 05 81
| 05 | 81 |
+----+----+-------------+-------- … --+   auth: must start 53 00
| 53 | 00 | L (u16 BE)  | JSON (L)      |   {"code":0,…}
+----+----+-------------+-------- … --+
+----+-----+------+------+--------------------+----------+   connect reply
| 05 | REP | FLAG | ATYP | bound addr (4|16)  | port u16 |
+----+-----+------+------+--------------------+----------+
```

- JSON code 10000004/75500002 → fatal session invalid. Any other non-zero code → failure.
- REP: `00` success, `01` server failure, `02` not allowed, `03` network unreachable, `04` host
  unreachable, `05` refused, `06` TTL expired, `07` command unsupported, `08` address type
  unsupported, other → failure. If REP ≠ 0, the remaining reply bytes are not read.
- On success: ATYP must be `01` or `04`; the address and port are read and discarded.
  **FLAG = 0x01 means "reusable"** (server agrees to the framed close protocol).
- Bytes following the reply are stream data and must stay buffered.

Auth reply details. The two bytes `53 00` are checked first, and a reply that starts otherwise
fails at once, without its length or JSON being read (contrast §7.2). Then:

| JSON part | Outcome |
|---|---|
| L = 0 (no JSON at all) | failure: the empty text is not valid JSON. This differs from §7.2, where L = 0 is accepted. |
| not valid JSON, or valid but not an object (array, string, number) | failure |
| an object with no `code` member | success, as if `code` were 0 |
| `code` present but not an integer | failure |
| `code` = 0 | success |

(The reference also lets the bare JSON value `null`, and a `code` of `null`, pass as success,
and it fails on a `message` that is not a string. Both are accidents of its decoder. An
implementation should reject a bare `null` and need not inspect `message`.) Members other than
`code` and `message` are ignored, and `message` serves only for the error text.

Sequencing. The init message and the destination message go out together in one write, and
nothing else is sent until the whole reply (server hello, auth reply, connect reply) has been
read and accepted. This holds in zero-RTT mode too: the reference never sends stream data ahead
of the reply. There, "zero-RTT" changes exactly three things: the RSV byte, the framing of the
data phase (§9.3), and, when the server also sets FLAG, the way the write side is closed (§9.3).
Whether a gateway accepts data frames sent before its reply, as the name suggests, is an **open
question** (§13). The 18 s deadline covers the reading of the three reply parts and is lifted
once they are in. Cancelling the caller's request during the handshake closes the connection.

### 9.2 Auth JSON (fields in order, compact, signed per §8.4)

| # | Key | Value |
|---|---|---|
| 1 | `sid` | current sid |
| 2 | `appId` | matched rule's appId |
| 3 | `url` | `tcp://` ‖ destAddr |
| 4 | `deviceId` | |
| 5 | `connectionId` | |
| 6 | `procHash` | HEX(SHA256(path)) |
| 7 | `userName` | username from onlineInfo |
| 8 | `rcAppliedInfo` | `0` |
| 9 | `lang` | `en-US` (fixed) |
| 10 | `destAddr` | `host:port` (IPv6 bracketed) |
| 11 | `destIP` | omitted when empty |
| 12 | `env` | §8.3 with platform `Linux`. The reference uses process `google-chrome-stable` / `/usr/bin/google-chrome-stable`, or `ssh` / `/usr/bin/ssh` when the destination port is 22 (cosmetic **(inferred)**). |
| 13 | `xRequestSig` | §8.4 |

**Destination rules**:

| Case | Wire host (destination message) | destAddr | destIP |
|---|---|---|---|
| IP rule | IP | `ip:port` | omitted |
| domain rule, addrPretend = true | **domain** (the server resolves it) | `domain:port` | omitted |
| domain rule, addrPretend = false | IP (resolved locally) | `domain:port` | IP |

The case is the kind of rule chosen in §6.6 step 5, and the values are those of §6.6:

- "IP" is the IPv4 address in dotted decimal: the literal the caller gave, or the address the
  name resolved to in §6.6 step 4. On the wire it is ATYP `01`.
- "domain" is the normalised name that was queried (§6.5), not the rule's own text. A query for
  `WWW.Example.NET.` under a rule `*.example.net` sends `www.example.net`. On the wire it is
  ATYP `03`.
- The IP-rule row also covers a name that resolved to an address when no domain rule was chosen
  (§6.6 step 5.2). The name is then sent nowhere.
- `appId` is the chosen rule's. `url` is always `tcp://` followed by the same text as
  `destAddr`.
- addrPretend exists on domain rules only (§6.2).

Where the other inputs come from. All are fixed at client setup except `sid`, and are the same
for every stream:

| Key | Source |
|---|---|
| `sid` | the session identifier of §5.2, or the one the latest refresh supplied (§5.8); read anew for each stream |
| `deviceId` | the device ID of §3.8: persisted, else configured, else generated at setup |
| `connectionId` | §3.8, computed **once** at client setup from the device ID and the time then; not per stream |
| `procHash` | §3.8 over the process path of row 12, so one of two constants (V8), and equal to `env…fingerprint` |
| `userName` | after a login: `data.username` of the E22 reply, verbatim (not the `user@domain` sent in E5). After a resume without login (§11.2): the user name from the client's configuration. The member is **always present**; when the name is empty it is sent as `"userName":""`. |
| `env` | constants only (row 12, §8.3); nothing in it comes from a server response |
| signing key | §3.8; on the login path always freshly random |

### 9.3 Data phase

**Zero-RTT disabled** (RSV = 0): raw mode. TLS application bytes are the TCP stream itself.
Half-close = TLS `close_notify` (write-side close).

**Zero-RTT enabled** (RSV = 1): framed mode, both directions:

```
+----+----+-----------+--------------+
| 01 | T  | len u16   | data[len]    |
+----+----+-----------+--------------+
T = 0x00 data   (len 0 frames are skipped; writers chunk at ≤ 0xFFFF)
T = 0x01 close  (len field ignored on receive → EOF; sent as 01 01 00 00)
other header → protocol error
```

- Write-side close: if "reusable" (FLAG = 1 **and** zero-RTT), send `01 01 00 00` and do
  **not** close the TLS write side. Otherwise use TLS write half-close. Close sends the
  write-close once, then closes TLS.
- Concurrent writers must not interleave frames.
- Framed reading, case by case:
  - A data frame with length 0 carries nothing and is passed over: the reader goes on to the
    next frame. It is neither end of stream nor an error.
  - A header whose first byte is not `01`, or whose second byte is neither `00` nor `01`, is a
    protocol error. The read fails, and the stream is unusable from then on.
  - A close frame ends the stream. Only its four header bytes are consumed. The reference does
    not remember that it has seen one, so a later read would look for another header; an
    implementation should keep reporting end of stream **(inferred)**.
  - The TLS connection ending exactly between two frames is a clean end of stream as well.
    Ending inside a header or inside a frame's data is an error (truncation). (The reference
    reports an end that falls right after a header, before the first data byte, as a clean one.
    That is an accident; treat it as truncation.)
- Raw reading: there is nothing in-band. End of stream is the end of the TLS connection's read
  side, that is the peer's `close_notify`. The reference's TLS library reports a transport
  closed without `close_notify` at a record boundary in the same way, so the reference cannot
  tell a truncated stream from a finished one; an implementation may tell them apart.
- A write of no bytes sends nothing in either mode.
- Connection pooling or reuse after close is **not** implemented by the reference (the
  `tun0rtt` pool parameters in policy are ignored). Whether the server allows reusing a closed
  stream's TLS connection is an **open question**.

---

## 10. Node selection

Source: `client/atrust/node.go`, `internal/ping/tcp.go`, `internal/ping/ping.go`, test
`node_test.go`.

Node choice is client policy. The reference client's method follows; an implementation may
choose another.

- For every group, probe all **WAN** nodes concurrently. Per node: 3 attempts, one every 500 ms
  (the first attempt fires after the first 500 ms tick). Each attempt is a TCP connect plus a
  full TLS handshake (SNI = host, no verify) with a **1 s** timeout. RTT = connect start to
  handshake done.
- Score = mean RTT of the successful attempts + (failed attempts × 1 s). A node is reachable if
  ≥ 1 attempt succeeded. The lowest score wins. Address strings are `host:port`.
- Groups with no reachable WAN node: probe their **LAN** nodes the same way.
- Groups with nothing reachable are absent from the result. Lookups then fall back to the major
  group.
- Re-probe periodically (reference default **300 s**, configurable). See §8.6 for eviction.

---

## 11. Client lifecycle (`sangfor-atrust-client`)

### 11.1 Capabilities

E1 is fetched first. If it fails but a cached manifest exists in the persistence blob, the
cache is used. With neither, setup fails. `zeroRTT = (data.capacities.zeroRTT.version != "")
AND data.options.tun0rtt.enable`. The policy's `sdpPolicy…tun0rtt` must **not** override it.

### 11.2 Setup sequence

1. Load the persistence blob (§11.3). Fill in the device ID and MACs from config if they are
   absent.
2. Manifest → zero-RTT flag.
3. **Resume** if sid, device ID and a stored policy document are all supplied. Skip login,
   restore cookies plus sid (§2.3), and compute a new connection ID. The sign key comes from
   config or is random.
   **Otherwise login**: generate a device ID or MAC if missing, a new connection ID and a
   random sign key. Run §5.2. Then fetch the policy (E23).
4. Snapshot the cookies. The sid must be non-empty.
5. Parse the policy (§6). Probe nodes (§10). Fetch the VIP (§7) from the best node of the major
   group, else any node. Exclude the VIP from the underlay.
6. Create the L3 tunnel object (connections are lazy). Persist the blob. Start the session
   refresh (§5.8) and node re-probe (§10) timers.
7. Teardown: cancel timers, close all L3 connections and fail pending flows.

### 11.3 Persistence blob (JSON, file mode 0600)

```
{"cookies":[{"host":"<host[:port]>","scheme":"https","name":"sid","value":"…"},…],
 "device_id":"<32 lowercase hex>",
 "mac_addresses":["xx:xx:xx:xx:xx:xx"],          (omitted if empty)
 "server_version_info":{…raw manifest JSON…}}    (omitted if empty)
```

---

## 12. Golden vectors

Every value below is *derived from the stated rule*: computed for this document, from the
sections cited, on inputs chosen for this document. None was captured from a server and none is
taken from the reference project's tests. The tools were Python 3 (`hashlib`, `hmac`, `base64`,
`json`, `struct`) and, for AES-128-CBC, `openssl enc -aes-128-cbc -K <key> -iv <iv>`. As a
check on the method, the same script first reproduced the challenge-check and `mitmSig` results
that the reference project's tests expect for their inputs. Addresses come from the RFC 5737 and
RFC 3849 documentation ranges, and host names are RFC 2606 names.

Shared inputs: `devicePubKeyMod` = `00C0FFEE5EED`, `devicePubKeyExp` = `65537`, `challenge` =
`example-challenge-0001`, ticket `TK-0001`, device ID `00112233445566778899aabbccddeeff`, sid
`S1D-example`, appId `app-0001`, tunnel sign key = the 32 bytes `00 01 02 … 1F`.

### V1. Challenge check (§3.3)

```
M   = SHA256("00C0FFEE5EED" ‖ "65537" ‖ C_chal)
    = E824BA865F056729BCD462B4530615D5260F61A23102685BE51DAB98AB272E41
key = E824BA865F056729BCD462B4530615D5, iv = 260F61A23102685BE51DAB98AB272E41
encryptedChallenge = 91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED
```

Any other `encryptedChallenge` string fails, including the same digits in lowercase.

### V2. Sign key (§3.3, same inputs)

```
F = 435658AA8C63A3131806A7EDFBE037283C415847D9D89DB4B0478B99CE1BBA1A
S = SHA256(HEX(F) ‖ "example-challenge-0001")
  = A4E29E6529F282EED3354B246B8759B7368B1D54F96F4304DA625376D383B7FD
K = E7B4C6CFA59121FDCB33ECC990676E9F0ACA451320B7DEB06A25D8EF1D980DE7
```

With no `antiMITMAttackData` at all (three empty strings):
`K0 = 6DCC2BEE653BAAB458A7CEE13821BD4A3D0B379603BD84AEF5E8070EAC5A3FD7`.

### V3. `mitmSig` (§3.3)

Raw object:
```json
{"enable":1,"antiMITMRequest":false,"devicePubKeyMod":"00C0FFEE5EED","devicePubKeyExp":"65537","rsaCert":"","sm2encCert":"","challenge":"example-challenge-0001","encryptedChallenge":"91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED","ticket":"TK-0001","mitmSig":"0e89931e17ea6ed910f9fd0e0491eb77b0f08f615ed4a7e97e696da8a04cf35f"}
```
Canonical string:
```
antiMITMRequest:false&challenge:example-challenge-0001&devicePubKeyExp:65537&devicePubKeyMod:00C0FFEE5EED&enable:1&encryptedChallenge:91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED&rsaCert:&sm2encCert:&ticket:TK-0001
```
`HEX(HMAC-SHA256(K, canonical)) = 0E89931E17EA6ED910F9FD0E0491EB77B0F08F615ED4A7E97E696DA8A04CF35F`,
with K from V2. The lowercase `mitmSig` above passes because it is uppercased before the
comparison.

### V4. Nonce generator (§3.3)

Starting from x = 20261116, the next 32 bytes are
`8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8`.

### V5. Certificate digest (§3.3)

The five bytes `30 03 02 01 05` stand in for a certificate's DER (only the hashing is
exercised). Their base64 is `MAMCAQU=`, and SHA256(`MAMCAQU=@~*&!()-`) =
`DAC223B577CE06F17C37839304EF8A94AE9EF3DA907EBFE09D6FB77B6E8771FB`.

### V6. Second-stage anti-MITM exchange (§3.6, E3)

Request body: the whitespace template of §4.3 with the V4 nonce and ticket `TK-0001`. It is 166
bytes long and its SHA-256 is
`24002AC288FCC0B3A38E87EB4657AB2430167DFB04E656D28DAEE2185B660DC6`.

Response body
`{"code":0,"data":{"nonce":"8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8","antiMITMEnable":1}}`
with K from V2 requires
`X-Response-Sig = EE78499E113699CC1D5326246C2C260268E2F22D1E63A27EC9054A5721FD809A`. A reply
with a different nonce, or with the header absent or different, fails the check.

### V7. reportEnv signature (§3.7, key K0)

```
message = "/controller/v1/public/reportEnv?clientType=SDPClient&lang=en-US&platform=Linux"
        ‖ {"deviceId":"00112233445566778899aabbccddeeff","env":{"endpoint":{"device":{"type":"browser"},"device_id":"00112233445566778899aabbccddeeff","mac_addresses":["02:00:5e:00:53:01"]}},"ticket":"TK-0001"}
X-Request-Sig = E2FDAB67D324A0A6B4A64EC447DC93F782F451DC09EAE08395CF228AD198E140
```

### V8. Identifiers and header encodings (§3.8)

- RID for base host `sdp.example.org:8443` = `c2RwLmV4YW1wbGUub3JnOjg0NDM=`
- `x-sdp-env` for the shared device ID =
  `eyJkZXZpY2VJZCI6IjAwMTEyMjMzNDQ1NTY2Nzc4ODk5YWFiYmNjZGRlZWZmIn0=`
- Connection ID for that device at Unix time 1794787200 s exactly =
  `B4377F7BABF2991B7D6983C4D3E19CD4-1794787200000000`
- procHash of `/usr/bin/zju-connect` = `51D7B8293B80675C346E3C78354EEB1979925693A276EA952CA2F04A6EF018A1`
- procHash of `/usr/bin/google-chrome-stable` = `AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19`
- procHash of `/usr/bin/ssh` = `D0195B967DFD056E32DA2FDA9C53251423FF5032D7A06AF76050F96433D0DCC3`

### V9. L3 flow auth request (§8.2–§8.4)

Inputs: the shared values, the V8 connection ID, conntrackHash 7, a TCP packet from
192.0.2.10:40000 to 203.0.113.5:443, lang `en-US`, platform `Linux`. Unsigned form (785 bytes):

```
{"sid":"S1D-example","appId":"app-0001","procHash":"51D7B8293B80675C346E3C78354EEB1979925693A276EA952CA2F04A6EF018A1","url":"tcp:203.0.113.5:443","deviceId":"00112233445566778899aabbccddeeff","connectionId":"B4377F7BABF2991B7D6983C4D3E19CD4-1794787200000000","rcAppliedInfo":0,"lang":"en-US","env":{"application":{"runtime":{"process":{"name":"zju-connect","digital_signature":"TrustAppClosed","platform":"Linux","fingerprint":"51D7B8293B80675C346E3C78354EEB1979925693A276EA952CA2F04A6EF018A1","description":"TrustAppClosed","path":"/usr/bin/zju-connect","version":"TrustAppClosed","security_env":"normal"},"process_trusted":"TRUSTED"}}},"conntrackHash":7,"ip":{"atype":2048,"protocol":6,"destAddr":"203.0.113.5","destPort":443,"srcAddr":"192.0.2.10","srcPort":40000},"xRequestSig":""}
```

`xRequestSig = 7A064BE0E5BBD4A6F7617B61D4F2B8A6EE44D21703A575912D5AAA450541FC56`. The signed
form is 849 bytes, so the frame on the wire is `05 13 03 51` followed by those bytes. Neither
`appToken` nor `domain` appears, and `"rcAppliedInfo":0` is present.

### V10. L3 data frame (§8.1)

Token `ab` and the four packet bytes `DE AD BE EF` give
`05 14 02 61 62 00 00 01 00 04 DE AD BE EF`. With an empty token the third byte is `00` and
`00 00 01` follows it directly. Token bytes are copied unchanged, spaces included.

### V11. L3 server frames (§8.1)

The byte stream `05 93 00 00 02 7B 7D 05 95 00 00` holds a flow auth response (status 0, JSON
`{}`) and then an empty heartbeat response. The result is the same however the bytes are split
across reads. A frame whose first byte is not `05`, or whose command is not one of those in
§8.1, closes the connection.

### V12. Session auth (§7)

- Request for the shared sid:
  `05 01 D0 53 00 00 15 {"sid":"S1D-example"} 05 04 00 01 00 00 00 00 00 00` (the JSON is 21
  bytes and starts at offset 7).
- Reply `05 D0 | 53 00 00 0A {"code":0} | 05 00 00 01 C0 00 02 21 00 00` → VIP 192.0.2.33. The
  third byte of the VIP header may hold any value.
- A reply whose status byte is non-zero fails, but only after all L bytes of its JSON have been
  read.

### V13. VIP update (§8.6)

`05 96 00 00 33 {"data":{"vip":"192.0.2.34","vip6":"2001:db8::22"}}` → IPv4 VIP 192.0.2.34 and
IPv6 VIP 2001:db8::22. Members other than `vip` and `vip6` are ignored.

### V14. TCP tunnel destination message (§9.1)

- `192.0.2.80:8080`, zero-RTT off → `05 01 00 01 C0 00 02 50 1F 90`; zero-RTT on →
  `05 01 01 01 C0 00 02 50 1F 90`
- `[2001:db8::5]:443` → `05 01 00 04 20 01 0D B8 00 00 00 00 00 00 00 00 00 00 00 05 01 BB`
- `app.example.net:443` → `05 01 00 03 0F 61 70 70 2E 65 78 61 6D 70 6C 65 2E 6E 65 74 01 BB`
- A name longer than 255 bytes is written in full with its length byte reduced mod 256.

### V15. TCP tunnel auth JSON (§9.2, §8.4)

Inputs: the shared values, the V8 connection ID, user name `user01`, destination
`app.example.net:443` reached through a domain rule with addrPretend = true (so `destIP` is
absent). Unsigned form (732 bytes):

```
{"sid":"S1D-example","appId":"app-0001","url":"tcp://app.example.net:443","deviceId":"00112233445566778899aabbccddeeff","connectionId":"B4377F7BABF2991B7D6983C4D3E19CD4-1794787200000000","procHash":"AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19","userName":"user01","rcAppliedInfo":0,"lang":"en-US","destAddr":"app.example.net:443","env":{"application":{"runtime":{"process":{"name":"google-chrome-stable","digital_signature":"TrustAppClosed","platform":"Linux","fingerprint":"AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19","description":"TrustAppClosed","path":"/usr/bin/google-chrome-stable","version":"TrustAppClosed","security_env":"normal"},"process_trusted":"TRUSTED"}}},"xRequestSig":""}
```

`xRequestSig = ACFDC0F8B848BF3A2583D127E11ABFC8F3DEA0C4EBA11BD3BA3CC816743FACC8`. The signed
form is 796 bytes, so the init message starts `05 01 81 53 03 03 1C`. It is followed by the
V14 destination message for `app.example.net:443`.

### V16. TCP tunnel server reply (§9.1)

`05 81 | 53 00 00 0A {"code":0} | 05 00 01 01 C0 00 02 50 1F 90` → connected, with the reusable
flag set. Every byte after these belongs to the data phase. A reply that does not begin
`05 81`, or whose second part does not begin `53 00`, fails the handshake.

### V17. TCP data frames (§9.3)

- The four bytes `ping` → `01 00 00 04 70 69 6E 67`. A reader must cope with a frame arriving
  in pieces of any size.
- A 65536-byte write becomes a frame `01 00 FF FF` with 65535 bytes, then `01 00 00 01` with the
  last byte.
- Close is sent as `01 01 00 00`. A receiver treats any frame beginning `01 01` as end of
  stream, whatever its length field holds.

### V18. Manifest (§11.1)

`{"code":0,"data":{"capacities":{"zeroRTT":{"version":"2.0"}},"options":{"tun0rtt":{"enable":true}}}}`
→ zero-RTT on. An empty `version`, or `enable` false, turns it off. A status other than 200 or
a non-zero `code` means the manifest was not obtained.

### V19. Next-step resolution and bodies (§5.5, §5.6)

- `{"nextService":"auth/token","nextServiceList":[{"authId":"a-17","authType":"auth/radius"}]}`
  → service `auth/radius`, authId `a-17`.
- `{"nextService":"","nextServiceList":[{"authId":"a-18"}]}` → `auth/sms`, mode B.
- `{"nextService":"auth/sendSms"}` → `auth/sms`, mode A.
- E17 TOTP body for code `000111` and user `user01@local`:
  `{"action":"auth","isPrevEffect":false,"skipSecondaryAuth":0,"totpToken":"000111","username":"user01@local"}`
  (member order is free; this body is not signed).
- Device binding: the first E21 body is `{"reason":"","unbindList":"[]"}`. A reply with
  `bindStatus` 1, `curBindNum` 3 and `bindNumLimit` 3 means one device must go. Choosing the
  device whose `id` is `dev-2` gives the second body `{"reason":"","unbindList":"[\"dev-2\"]"}`.

### V20. Captcha answer and external login (§5.4)

- Points (40,25) and (120,60) on a 320×160 image → `graphCheckCode` content
  `{"coordinates":[[40,25],[120,60]],"width":320,"height":160}`.
- CAS callback `https://sdp.example.org:8443/passport/v1/auth/cas?sfDomain=local&ticket=ST-0001`,
  fetched without following redirects, must answer 302. With
  `Location: https://sdp.example.org:8443/portal/shortcut.html?data=%7B%22ticket%22%3A%22PT-0001%22%7D`
  the portal ticket is `PT-0001`. A `Location` on another host or path, or with an empty
  ticket, fails.

### V21. Policy document (§6)

With the server configured as `sdp.example.org`:

```json
{"data":{
 "appList":{"data":{
  "appInfo":[{"apps":[
   {"id":"app-0001","nodeGroupId":"ng-1","accessModel":"L3VPN","enableTCPPrefL3":true,
    "addressList":[{"protocol":"all","port":"1-65535","host":"198.51.100.0/24"}]},
   {"id":"app-0002","nodeGroupId":"ng-2","accessModel":"L3VPN","addrPretend":false,
    "addressList":[{"protocol":"tcp","port":"443","host":"app.example.net","ip":["203.0.113.5"]},
                   {"protocol":"udp","port":"5000-5010","host":"*.example.net"}]},
   {"id":"app-0003","nodeGroupId":"ng-1","accessModel":"WEB",
    "addressList":[{"protocol":"tcp","port":"80","host":"203.0.113.9"}]}]}],
  "config":{"nodeGroupConf":{
   "majorNodeGroup":{"id":"ng-1"},
   "nodeGroupList":[
    {"id":"ng-1","addressInfo":[{"address":"{{sdpcHost}}","type":"WAN"},
                                {"address":"198.51.100.200:4441","type":"lan"}]},
    {"id":"ng-2","addressInfo":[{"address":"node2.example.org:441","type":"wan"}]}]}}}},
 "sdpPolicy":{"data":{"clientOption":{
  "dnsOption":{"firstDNS":"","secondDNS":""},
  "dnsOptionV2":{"firstDNS":"192.0.2.53","secondDNS":"192.0.2.54"}}}}}}
```

Expected result:

- IP rule 198.51.100.0–198.51.100.255, ports 1–65535, protocol `all`, app-0001, group ng-1, TCP
  prefers L3.
- Domain rule `app.example.net`, port 443, tcp, app-0002, group ng-2, addrPretend false. Its
  pinned address adds the DNS pin `app.example.net` → 203.0.113.5 and an IP rule for 203.0.113.5
  with the same port, protocol and application.
- Domain rule for the suffix `.example.net`, ports 5000–5010, udp, app-0002.
- app-0003 is ignored (`WEB`).
- Node groups: ng-1 has WAN node `sdp.example.org:441` and LAN node `198.51.100.200:4441`, and
  is the default group. ng-2 has WAN node `node2.example.org:441`. The address 198.51.100.200 is
  kept out of the routed destinations although the app-0001 range covers it.
- DNS servers: 192.0.2.53 (primary) and 192.0.2.54, taken from `dnsOptionV2` because
  `dnsOption.firstDNS` is empty.

### V22. Node scoring (§10)

Node A answers two of three probes, in 30 ms and 50 ms: score 40 ms + 1 s = 1.04 s. Node B
answers all three, in 90, 100 and 110 ms: score 100 ms. B is chosen. A node that answers no
probe is unreachable.

### V23. Domain selection (§6.5)

Domain rules, in document order:

| # | Policy host | Rule name | Protocol | Ports | appId |
|---|---|---|---|---|---|
| 1 | `example.org` | `example.org` | tcp | 443 | app-a |
| 2 | `*.example.org` | `.example.org` | tcp | 1–65535 | app-b |
| 3 | `*.DEV.example.Org.` | `.dev.example.org` | tcp | 8000–8999 | app-c |
| 4 | `www.example.org` | `www.example.org` | udp | 53 | app-d |

| Query | Candidate names, in order | Result |
|---|---|---|
| `example.org`, tcp 443 | `example.org` | app-a |
| `example.org`, tcp 80 | `example.org` | none: rule 2 does not cover the bare name |
| `www.example.org`, tcp 443 | `www.example.org`, `.example.org`, `example.org` | app-b: rule 4 fails the test, rule 2 is next |
| `Www.EXAMPLE.org.`, tcp 443 | the same | app-b |
| `www.example.org`, udp 53 | the same | app-d |
| `api.dev.example.org`, tcp 8080 | `.dev.example.org`, `.example.org`, `example.org` | app-c |
| `api.dev.example.org`, tcp 443 | the same | app-b: rule 3 fails on the port |
| `dev.example.org`, tcp 8080 | `.example.org`, `example.org` | app-b: rule 3 does not cover the bare name |
| `badexample.org`, tcp 443 | none | none |

### V24. Port and host strings (§6.2)

| `port` | Result |
|---|---|
| `443` | 443–443 |
| `8000-8100` | 8000–8100 |
| empty string, ` 443` (leading space), `80-`, `1-2-3` | address skipped |
| `9000-8000` | kept as 9000–8000; contains no port |
| `0` | 0–0 |
| `1-70000` | kept as 1–70000; contains every port from 1 up |

| `host` | Result |
|---|---|
| `192.0.2.7` | IP rule 192.0.2.7–192.0.2.7 |
| `198.51.100.77/24` | IP rule 198.51.100.0–198.51.100.255 |
| `203.0.113.9/32` | IP rule 203.0.113.9–203.0.113.9 |
| `0.0.0.0/0` | IP rule 0.0.0.0–255.255.255.255 |
| `192.0.2.16-192.0.2.31` | IP rule 192.0.2.16–192.0.2.31 |
| `192.0.2.31-192.0.2.16` | a rule that matches nothing |
| ` 192.0.2.7` (leading space) | domain rule named `192.0.2.7` |
| `2001:db8::24` | skipped |
| `*.example.org` | domain rule, suffix `.example.org` |
| `a*.example.org` | dropped |

An address with protocol `TCP`, and an app with accessModel `l3vpn`, yield nothing.

### V25. Rule order with pinned addresses (§6.2, §6.5)

Three apps in this order, each with one address, all `L3VPN`:

- app-0010: tcp, port `1-1023`, host `203.0.113.0/24`
- app-0011: tcp, port `443`, host `db.example.net`, `ip` =
  `["203.0.113.20","2001:db8::20","203.0.113.21"]`
- app-0012: tcp, port `443`, host `203.0.113.21`

IP rule sequence: (0) 203.0.113.0–203.0.113.255, ports 1–1023, app-0010; (1) 203.0.113.20, port
443, app-0011; (2) 203.0.113.21, port 443, app-0011; (3) 203.0.113.21, port 443, app-0012. The
IPv6 entry adds nothing. Pins: `db.example.net` → 203.0.113.20, 203.0.113.21.

| IP lookup, tcp | First match | Last match |
|---|---|---|
| 203.0.113.20 port 443 | app-0010 | app-0011 |
| 203.0.113.21 port 443 | app-0010 | app-0012 |
| 203.0.113.21 port 80 | app-0010 | app-0010 |
| 203.0.113.99 port 443 | app-0010 | app-0010 |

Three successive resolutions of `db.example.net` from the pins (§6.6 step 4) give 203.0.113.20,
203.0.113.21, 203.0.113.20.

### V26. From destination to request values (§6.6, §9.2)

With the policy of V21, zero-RTT off, and every best node known. app-0002 has no
`enableTCPPrefL3`, so it is false there; app-0001 has it true.

| Destination | Rule chosen | appId, group | Destination message | `destAddr` | `destIP` |
|---|---|---|---|---|---|
| `app.example.net:443` | domain rule `app.example.net`, addrPretend false; address 203.0.113.5 from the pin | app-0002, ng-2 | `05 01 00 01 CB 00 71 05 01 BB` | `app.example.net:443` | `203.0.113.5` |
| `APP.Example.NET.:443` | the same; the name is normalised | app-0002, ng-2 | the same | `app.example.net:443` | `203.0.113.5` |
| `203.0.113.5:443` | IP rule made by the pin | app-0002, ng-2 | `05 01 00 01 CB 00 71 05 01 BB` | `203.0.113.5:443` | omitted |
| `www.app.example.net:443`, resolved to 203.0.113.77 by the outside resolver (the policy DNS server 192.0.2.53 is covered by no rule here, so it cannot be reached through the VPN) | domain rule `app.example.net` (a plain name covers the names beneath it); no pin has this name | app-0002, ng-2 | `05 01 00 01 CB 00 71 4D 01 BB` | `www.app.example.net:443` | `203.0.113.77` |
| `x.example.net:443`, resolving to an address no IP rule covers | none: the only candidate is the udp rule | — | — | — | — |
| `198.51.100.9:22`, take-all (§6.6) not set | none for the TCP tunnel: the one matching rule prefers L3 | — | — | — | — |
| `198.51.100.9:22`, take-all set | that IP rule | app-0001, ng-1 | `05 01 00 01 C6 33 64 09 00 16` | `198.51.100.9:22` | omitted |
| `[2001:db8::5]:443` | none: IPv6 literal | — | — | — | — |

In every row with a rule, `url` is `tcp://` followed by the `destAddr` text. The last tunnelled
row has port 22 and therefore the `ssh` process values (§9.2 row 12). An IP lookup for the node
address 198.51.100.200, tcp port 4441, still returns the app-0001 rule (§6.4).

### V27. TCP tunnel reply and frame edge cases (§9.1, §9.3)

After a server hello `05 81`:

- `53 00 00 00` → handshake failure (no JSON).
- `53 00 00 02 7B 7D` (the JSON `{}`) → accepted; the connect reply is read next.
- `53 00 00 10` followed by the 16 bytes `{"message":"ok"}` → accepted.
- `53 00 00 01 78` (the single byte `x`) → handshake failure.
- `53 01 …` → handshake failure, with nothing more read.

Framed data phase: the received bytes `01 00 00 00 01 00 00 02 68 69 01 01 00 00` deliver the two
bytes `hi` and then end of stream; the empty first frame delivers nothing. Received bytes
starting `03 00 00 00`, or `01 02 00 00`, are a protocol error.

### V28. Password login details (§5.2, §5.4)

- User `user01` with domain `local` → `username` `user01@local`. User `a@b` with domain `local`
  → `a@b@local`.
- Password `pw` with `antiReplayRand` `r1` → the plaintext encrypted per §3.4 is the five bytes
  `pw_r1`.
- An E5 reply `{"code":0,"data":{"ticket":"TK-0001","nextService":""}}` → ticket `TK-0001`; after
  E10 the chain starts with E11. With `"nextService":"auth/totp"` instead, the chain starts with
  the `auth/totp` step and no E11 precedes it.
- An E22 reply `{"code":0,"data":{"username":""}}` → login succeeds, and the tunnel auth JSON
  carries `"userName":""`.

### Rules stated elsewhere instead of as vectors

- Session refresh outcomes (invalid or transient): §5.8 is the complete rule.
- Tunnel auth replies succeed or fail on `code` (and the status byte) alone, never on the
  `message` text (§7.2, §9.1).
- `addrPretend` is false only for the JSON boolean `false` (§6.2).
- The TCP tunnel takes the last matching TCP rule, the L3 tunnel the first (§6.5).
- SOCKS5-style REP codes other than `00` fail the connect (§9.1).

---

## 13. Open questions and server-version variance

1. **TLS suite actually negotiated** by gateways (TLS 1.2 vs 1.3, RSA vs ECDSA certificates).
   It determines the minimum `tls` crate scope. A capture against a real gateway would resolve
   it.
2. **GM/SM2 TLS (TLCP)**: `sm2encCert` exists, so some deployments may offer national-crypto
   TLS. This client never negotiates it and only hashes the text. Whether any gateway *requires*
   TLCP is unknown.
3. **Anti-MITM stage-2 trigger polarity**: the second-stage request is performed when
   `antiMITMRequest == false`. The flag may mean "already done" rather than "please do". This
   needs confirmation against the official client.
4. **Anti-MITM enforcement**: the reference never aborts on failure, and the vendor
   clients reportedly do not either. Whether the server ever refuses tunnels after a failed or absent stage 2 is unknown.
5. **`xRequestSig` verification**: the sign key is random and never sent, so the server cannot
   verify it unless the key is derived from something shared. A configurable key exists ("sign
   key" option), which suggests some deployments supply a real one. The derivation in the
   official client is unknown.
6. **L3 data `count` > 1** upstream, and whether downstream 0x94 ever uses the same header
   form. Observed downstream is a raw packet stream.
7. **VIP data trailing 2 bytes** (port?) and the meaning of header byte XX (`0x00`/`0x01`/`0x7F`).
   The reference also carries an unused parser for a 6-byte form `?? 01 a b c d`, possibly an
   older reply format.
8. **Session-auth CMD 0x04 / ATYP 1** semantics, and whether IPv6-only or dual-stack requests
   use another ATYP.
9. **Heartbeat liveness**: as implemented, a successful probe write prevents the miss counter
   from ever reaching 3, so silent peer death is detected only by TCP errors. The vendor client
   reportedly behaves the same way. An implementation may add an opt-in read-idle watchdog.
10. **TCP zero-RTT reuse**: the reference marks connections "reusable" and sends the framed
    close, but never re-uses a TLS connection. The pooling semantics (`preConnNum`,
    `minIdleConnNum`, `maxIdleConnNum`, `maxIdleLingerTime`) are unspecified.
11. **Policy JSON key casing** on real servers (`nodeGroupId` vs `NodeGroupID`, etc.). Use
    case-insensitive matching.
12. **Node address default port** `:441` is appended whenever no `:` is present, which breaks
    bare IPv6 literals.
13. **Error code meanings**: 75500401 and 75500000 are inferred. A fuller code table would need
    server documentation or captures.
14. **Locale**: L3 auth `lang` comes from the environment and the TCP tunnel's is fixed at
    `en-US`. It is unknown whether the server cares.
15. **`mod=1` / `needTicket`** semantics beyond the observed sequencing.
16. **SMS mode selection**: the mode is inferred from the presence of `authId`. One gateway
    family is known for each mode. Other variants may exist.
17. **gzip**: whether gateways compress when `Accept-Encoding` is absent. Expected not to
    **(inferred)**.
18. **TCP zero-RTT early data**: whether a gateway accepts data frames sent before its
    handshake reply. The reference always waits for the reply (§9.1), so its "zero-RTT" saves
    no round trip.
19. **Empty `antiReplayRand`**: what the server answers to a password encrypted with an empty
    value, and whether authConfig can legitimately omit it (§5.2).
20. **Default login domain**: which `authServerInfoList` entry the vendor client uses when the
    user names no domain (§5.4).
21. **Policy value casing**: whether `protocol` and `accessModel` ever arrive in another letter
    case than `tcp`/`udp`/`all` and `L3VPN` (§6.2).
22. **TCP tunnel to an IPv6 destination**: the wire format has ATYP `04`, but no policy rule can
    match an IPv6 address and the reference never sends one (§6.6).
23. **Name form in `destAddr`**: the reference always sends the lower-cased name without a
    trailing dot (§9.2). Whether the gateway compares it with the policy host exactly or
    case-insensitively is unknown.

### Shared with EasyConnect (coordinate with that spec)

Underlay dialer (L0), TLS client (L1), HTTP/1.1 + cookie jar (L2/L3), the IP/domain resource
model and first/last interval matching (§6.5), the user-prompt abstraction (code /
captcha / external login), NSS key logging, and the routed-IP-set and fake-IP consumers
(`internal/ippool` is a stack-side fake-IP allocator, not part of either protocol).
