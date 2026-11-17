# Sangfor EasyConnect (legacy SSL VPN) — clean-room protocol specification

Status: draft. Written clean-room from the behaviour of the open-source client
`Mythologyli/zju-connect` at commit `8576e7a16076c4175d6dbe276573a353cf7260be`, with no code or
comments copied. File paths are cited for traceability only. Statements marked **(inferred)**
were not read directly; **open question** marks unknowns.

Scope: EasyConnect only. aTrust (the newer Sangfor product) is specified separately; parts
shared with it are flagged "shared with aTrust".

Conventions: all multi-byte integers are big-endian unless stated. "TwfID" is the server-issued
session identifier (an ASCII string). `‖` is concatenation.

---

## 1. Component layer map

Bottom-up. "Generic" means reusable outside Sangfor and belongs in its own crate; "Sangfor"
means EasyConnect-specific. Existing rsl crates are listed as dependencies and not re-specified.

| # | Component (proposed crate) | What it does | Key wire/format facts | Kind | Depends on |
|---|---|---|---|---|---|
| L0 | Underlay socket dialer (platform, not protocol) | Opens TCP to the gateway via the physical interface, never via the VPN's own TUN; told the assigned virtual IP so it can exclude that interface | Plain TCP connect; 10 s connect timeout when no custom dialer; TCP keepalive 30 s | Generic (shared with aTrust) | OS sockets |
| L1a | TLS client, **modern profile** (`tls` crate, generic) | HTTPS for the web API and the token probe | TLS 1.2 (server-side; client offers up to 1.3); certificate **not verified**; must expose the ServerHello `session_id`; optional client certificate presented via **one server-initiated renegotiation** (RFC 5746 secure renegotiation, client side) | Generic | rsl `crypto`, `pki` |
| L1b | TLS client, **legacy profile** (same `tls` crate, feature `legacy`) | Carries the binary tunnel channels (`L3IP`, `TCPP`) | TLS **1.1 only**; single cipher suite `TLS_RSA_WITH_RC4_128_SHA` (0x0005) + SCSV 0x00FF; null compression; **caller-chosen 32-byte ClientHello session_id** (channel selector); exactly one extension, heartbeat (0x000F); no SNI; no verification | Generic mechanism, Sangfor-driven quirks (exposed as ClientHello builder hooks) | rsl `crypto-legacy` (RC4, MD5, SHA-1, RSA PKCS#1 v1.5 encrypt), `crypto` (HMAC) |
| L2 | HTTP/1.1 client codec (`http1` crate) | Request serialisation, response parsing, pipelining of raw requests | GET/POST, `Host`, `Cookie`, `User-Agent`, `Content-Length`; response: status line, headers, `Content-Length` and `chunked` bodies, optional gzip; keep-alive reuse; no cookie jar needed | Generic | L1a |
| L2b | `application/x-www-form-urlencoded` encoder (inside `http1` or tiny crate) | Login form bodies | Keys sorted ascending, `&`-joined, percent-encoding per WHATWG form rules | Generic | — |
| L3 | Lenient XML extractor (`xml-lite` crate) | Read element text and element attributes from server replies | Documents are small, UTF-8, mostly single-line; need: first-match element text by name, child-element navigation, attribute lookup | Generic | — |
| L4a | RSA password wrapper (`sangfor-ec-auth`, module) | Encrypts the login password | Modulus = hex string, exponent = decimal string (default 65537); plaintext = password ‖ `_` ‖ CSRF code; RSAES-PKCS1-v1_5; output lowercase hex | Sangfor (thin over rsl RSA) | rsl `crypto-legacy` RSA PKCS#1 v1.5 |
| L4b | TOTP (`totp` crate) | Second factor when server asks for a token | RFC 6238: Base32 secret, HMAC-SHA-1, 30 s step, 6 digits **(inferred: library defaults)** | Generic | rsl `crypto` HMAC/SHA-1 |
| L4c | PKCS#12 reader (`pkcs12`, in `pki` group) | Loads client cert + key for certificate login | Password-protected `.p12`; first cert + private key | Generic | rsl `asn1`, `x509`, `crypto` (PBE ciphers, possibly legacy RC2/3DES) |
| L5 | Auth/login state machine (`sangfor-ec-auth`) | Obtains an authorised TwfID | `/por/login_auth.csp`, `/por/rand_code.csp`, `/por/login_psw.csp`, `/por/login_sms.csp`, `/por/login_sms1.csp`, `/por/login_token.csp`, `/com/server.crt`, `/por/login_cert.csp` | Sangfor | L2, L2b, L3, L4a–c |
| L6 | Config/resource parsing (`sangfor-ec-resource`) | Line list, route/ACL resources, DNS rules, DNS servers, VIP-allocation flag | `/por/conf.csp` → `<Conf><Mline list=…>`; `/por/rclist.csp` → `<Resource><Rcs><Rc …>`, `<Dns data dnsserver>`, `<Other allocateVip>` | Sangfor (resource model shared with aTrust) | L2, L3 |
| L7a | Tunnel token derivation (`sangfor-ec-tunnel`) | Builds the 48-byte tunnel token | Hex of server TLS session_id (first 31 chars) ‖ 0x00 ‖ TwfID (16 chars) | Sangfor | L1a, L2 |
| L7b | L3 tunnel handshake + framing (`sangfor-ec-tunnel`) | Virtual IP request; separate send and receive channels carrying raw IPv4 packets | 64-byte request messages with opcode 0x00/0x05/0x06; one IP packet per TLS record; no extra framing | Sangfor | L1b, L7a |
| L7c | TCP-proxy tunnel (`sangfor-ec-tunnel`) | Per-connection TCP relay when no virtual IP is allocated | Session-id `TCPP`; CRLF text commands `C01`/`C02`/`C03`, replies `Cxx OK`, then raw byte stream | Sangfor | L1b |
| L8 | Session orchestration (`sangfor-ec-client`) | Line selection, ordering/timing, session keepalive, IP-request-channel pinning, channel re-establishment, error policy | `/por/update_session.csp` every 60 s; ≥ 1 s gap between token and IP request; 5-retry channel recovery | Sangfor | L5–L7 |
| — | L3 packet codecs | Inspect/route IPv4/TCP/UDP/ICMP packets entering/leaving the tunnel | IPv4 only; local MTU 1400, inbound buffer 1500 | Generic | rsl `ip`, `tcp`, `udp`, `icmp`, `dns` (existing) |
| — | Auth challenge UI hook (interface only) | Ask the user for captcha text / SMS code / TOTP code | Captcha: image bytes in, non-blank text out | Generic interface (shared with aTrust) | — |

Not needed by EasyConnect: JSON, SM2/SM3/SM4, any TLS 1.3-only feature, HTTP/2, cookie jars,
redirect handling **(inferred: servers observed do not redirect API calls; see §9)**.

---

## 2. Transport (L0) and TLS (L1)

### 2.1 Underlay

- Server address is `host:port` (default port 443). All connections — HTTPS API, token probe,
  tunnel channels, line-latency probes — go to this one endpoint (or the selected line, §6.3).
- After the virtual IP is assigned the underlay must stop considering any interface that carries
  that IP (prevents the tunnel being routed into itself). `underlay/dialer.go`

### 2.2 TLS modern profile (HTTPS API and token probe)

Facts (`client/easyconnect/client.go`, `request.go`):

- Server certificate is **never validated** (no chain, no hostname check). An implementation
  should offer optional pinning, but the protocol does not require validation.
- Client offers a mainstream modern ClientHello (TLS 1.2 + 1.3, ECDHE suites, SNI = server host).
  Servers are believed to answer with TLS 1.2 **(inferred)**: the token derivation (§7.1) needs a
  server-chosen ServerHello `session_id`, which only TLS ≤ 1.2 provides meaningfully.
- The TLS stack must expose the raw ServerHello `session_id` bytes to the caller after handshake.
- Client-certificate login (§5.6) needs:
  - presenting a client certificate (RSA or EC key from PKCS#12),
  - accepting **one** server-initiated renegotiation (HelloRequest) per connection, with
    RFC 5746 `renegotiation_info`; the server requests the certificate during that renegotiation
    **(inferred from the "renegotiate once as client" setting being required)**.
- NSS key-log output is supported for debugging (optional).

Minimum TLS 1.2 subset recommended for an rsl `tls` crate: ECDHE-RSA/ECDHE-ECDSA with
AES-GCM, plus `TLS_RSA_WITH_AES_128_CBC_SHA` as fallback **(inferred: legacy appliances)**;
extensions SNI, supported_groups, ec_point_formats, signature_algorithms, renegotiation_info,
extended_master_secret. Session resumption not needed.

### 2.3 TLS legacy profile (tunnel channels)

Every tunnel channel (`L3IP` and `TCPP`) opens a fresh TCP connection and performs a TLS
handshake with this exact ClientHello (`client/easyconnect/protocol.go`):

| Field | Value |
|---|---|
| Record layer version | 0x0301 **(inferred: TLS stack default for first flight)** |
| `client_version` | 0x0302 (TLS 1.1) |
| `random` | 32 random bytes |
| `session_id` | length 32; bytes = ASCII channel tag (`L3IP` or `TCPP`) followed by 28 zero bytes |
| `cipher_suites` | `00 05` (TLS_RSA_WITH_RC4_128_SHA), `00 FF` (TLS_EMPTY_RENEGOTIATION_INFO_SCSV) |
| `compression_methods` | `00` only |
| extensions | exactly one: type `00 0F` (heartbeat), length `00 01`, data `01` (peer_allowed_to_send) |

Role of the tag: tunnel channels and the web API arrive at one listener on the same port, and
the tag at the start of the ClientHello `session_id` is what directs a connection to the tunnel
service. Absence of SNI and of other extensions is part of the fingerprint the server expects
**(inferred: only this hello is known to work)**.

Negotiated parameters (fixed by the offer): TLS 1.1, RSA key transport, RC4-128 stream cipher,
HMAC-SHA-1 record MAC, TLS 1.0/1.1 PRF (MD5 ⊕ SHA-1). Record layer needs no explicit IV
(stream cipher). Server certificate is not validated. The server's ServerHello `session_id` on
these channels is not used.

Required TLS-1.1-legacy subset for the rsl `tls` crate: RSA key exchange (PKCS#1 v1.5
encryption of the 48-byte pre-master secret), RC4_128_SHA record protection, TLS 1.0/1.1 PRF,
Finished verification, a ClientHello builder allowing caller-specified session_id and a raw
extension list. No renegotiation, no resumption.

Timeouts: the whole handshake plus the channel's request/response exchange is bounded by a
15 s deadline (the reference client's value; an implementation may choose its own). After
success the deadline is cleared.

---

## 3. HTTP/1.1 (L2) — required subset

Observed client behaviour (`client/easyconnect/request.go`, `client.go`):

- Methods: `GET`, `POST`. URL scheme always `https`, host = configured `host:port`.
- Headers the protocol depends on:
  - `Cookie: TWFID=<TwfID>` set **manually** on every authenticated request (no cookie jar;
    `Set-Cookie` responses are ignored).
  - `User-Agent: EasyConnect_windows` on captcha, login_psw, SMS, TOTP, cert requests;
    `User-Agent: EasyConnect_Linux_Ubuntu` on update_session; the initial `login_auth`,
    `conf.csp`, `rclist.csp` requests send the HTTP library's default UA. Whether servers check
    UA is an **open question**; an implementation should send `EasyConnect_windows` by default.
  - POST bodies are url-encoded forms, sent **without** a `Content-Type` header (only
    `Content-Length`). Some POSTs have an empty body (`Content-Length: 0`). Whether servers need
    `Content-Type: application/x-www-form-urlencoded` is an **open question**; sending it is
    believed harmless **(inferred)**.
  - The reference HTTP stack also adds `Accept-Encoding: gzip` and transparently decompresses;
    a minimal client can omit `Accept-Encoding` and then need not decode gzip.
- Response handling: status line + headers; body by `Content-Length` or `Transfer-Encoding:
  chunked`. Status code is only checked for `update_session` (200 ok, 404 = unsupported).
  Bodies are treated as text and searched for markers (§5).
- Connection reuse (keep-alive) is used but not required.
- Timeouts: 30 s per whole request, 15 s to response headers.
- Redirects: the reference library follows up to 10 automatically. **Open question** whether
  any endpoint redirects.

### 3.1 Raw pipelined request (token probe)

The token probe (§7.1) writes, on a single fresh TLS connection, two pipelined requests in one
write, with no other headers:

```
GET /por/conf.csp HTTP/1.1\r\n
Host: <host:port>\r\n
Cookie: TWFID=<TwfID>\r\n
\r\n
GET /por/rclist.csp HTTP/1.1\r\n
Host: <host:port>\r\n
Cookie: TWFID=<TwfID>\r\n
\r\n
```

It then reads at least 1 byte (up to 8) of response and closes. Response content is discarded;
the purpose is to make the server bind the TLS session to the TwfID **(inferred)**. The HTTP
crate therefore needs a "write raw request bytes" escape hatch, or the probe is done directly on
the TLS stream.

---

## 4. Body formats (L3)

All server replies are XML-ish text. The reference client mostly uses substring/first-match
regex extraction, not a schema; an implementation should use a lenient XML reader and fall back
to substring search.

Extraction primitive needed: **text of the first `<Name>…</Name>` element** (matching within a
line, greedy to the last close tag on that line — in practice equals the element text), and
**attribute value of a named element under a path**.

Form encoding: `application/x-www-form-urlencoded`, keys in ascending byte order.

---

## 5. Authentication state machine (L5)

### 5.1 Endpoints

| Step | Method | Path + query | Cookie | Body (form fields) |
|---|---|---|---|---|
| Pre-auth | GET | `/por/login_auth.csp?apiversion=1` | none | — |
| Captcha image | GET | `/por/rand_code.csp?apiversion=1` | TWFID | — |
| Password | POST | `/por/login_psw.csp?anti_replay=1&encrypt=1&type=cs` | TWFID | `mitm`=`""`, `svpn_name`, `svpn_password` (hex RSA), `svpn_rand_code` (captcha or `""`), `svpn_req_randcode` (CSRF code or `""`) |
| SMS send | POST | `/por/login_sms.csp?apiversion=1` | TWFID | empty |
| SMS verify | POST | `/por/login_sms1.csp?apiversion=1` | TWFID | `svpn_inputsms` |
| TOTP | POST | `/por/login_token.csp` | TWFID | `svpn_inputtoken` |
| Server CA | POST | `/com/server.crt` | TWFID | empty |
| Cert login | POST | `/por/login_cert.csp?anti_replay=1&encrypt=1&type=cs` | TWFID | empty |
| Config | GET | `/por/conf.csp` | TWFID | — |
| Resources | GET | `/por/rclist.csp` | TWFID | — |
| Keepalive | GET | `/por/update_session.csp?apiversion=1&twfid=<TwfID>` | TWFID | — |

(Query parameters for update_session are encoded in sorted order: `apiversion=1&twfid=…`.)

### 5.2 Pre-auth reply (`login_auth.csp`)

Elements read from the body:

| Element | Required | Meaning |
|---|---|---|
| `TwfID` | yes | pre-auth session id; used as cookie from now on |
| `RSA_ENCRYPT_KEY` | yes | RSA modulus, hex (case-insensitive), big-endian |
| `RSA_ENCRYPT_EXP` | no | RSA public exponent, decimal; default 65537 |
| `CSRF_RAND_CODE` | no | anti-CSRF nonce; if present it is appended to the password and echoed in `svpn_req_randcode` |
| `RndImg` | yes in practice | `1` → captcha required, otherwise not |
| `VPNVERSION` | no | informational server version string |

### 5.3 Password encryption (L4a)

```
plaintext  = UTF-8(password)                       if no CSRF code
           = UTF-8(password) ‖ "_" ‖ CSRF code      otherwise
ciphertext = RSAES-PKCS1-v1_5-Encrypt(n = hex(RSA_ENCRYPT_KEY), e = RSA_ENCRYPT_EXP, plaintext)
svpn_password = lowercase hex(ciphertext)   (length = 2 × modulus bytes)
```

Randomised padding — no fixed test vector possible; verify by round-trip with a test key.

### 5.4 Captcha

If `RndImg` = `1`: GET `rand_code.csp`, body is an image (format not checked; typically
JPEG/GIF **(inferred)**). Hand to the challenge UI; reject an empty/whitespace-only answer.
The answer goes in `svpn_rand_code`.

### 5.5 Password reply classification (`login_psw.csp`)

The body text is tested against the rows below from the top, and the first row that applies
decides the outcome:

| # | Body contains | Outcome |
|---|---|---|
| 1 | `<NextService>auth/sms</NextService>` or `<NextAuth>2</NextAuth>` | **SMS** second factor (§5.6) |
| 2 | `<NextService>auth/token</NextService>` or `<NextAuth>7</NextAuth>` | **TOTP** second factor |
| 3 | `<NextAuth>0</NextAuth>` | **certificate** second factor |
| 4 | a `<NextAuth>` element with any value other than `-1` | failure: this second factor is not supported |
| 5 | `<Result>1</Result>` | authorised. A `<TwfID>` element, when present, replaces the current TwfID. |
| 6 | none of the above | failure: the credentials were not accepted |

A failure should carry the reply body to the caller, since it holds the server's explanation.

Known `NextAuth` codes: −1 none, 0 certificate, 2 SMS, 7 TOTP. Others **open question**.

### 5.6 Second factors

**SMS**
1. POST `login_sms.csp` (empty body). Success iff body contains any of
   `验证码已发送到您的手机` (UTF-8: "code sent to your phone"), `请输入验证码：`
   ("please enter the code:", full-width colon U+FF1A), or `<USER_PHONE>`. Else error.
2. Ask user for the code; POST `login_sms1.csp` with `svpn_inputsms`.
3. Success iff body contains `Auth sms suc` or `欢迎访问` ("welcome"). Update TwfID if a
   `<TwfID>` element is present.

**TOTP**
1. Code = configured secret → RFC 6238 now, else ask the user.
2. POST `login_token.csp` with `svpn_inputtoken`.
3. Success iff body contains `Totp auth succ`; the body **must** contain `<TwfID>`, which
   replaces the current TwfID.

**Certificate** (only reached after password step returned NextAuth 0)
1. POST `/com/server.crt` → PEM bundle; must parse as ≥ 1 certificate else error. (It is
   loaded as a trust root, but verification stays disabled, so it is effectively a liveness
   check.)
2. Replace the HTTPS client config: client certificate from PKCS#12, allow one renegotiation,
   still no verification. All later HTTPS requests use this config.
3. POST `login_cert.csp` (empty body). Success iff body contains `<Result>1</Result>`,
   `Login successfully`, or `Auth cert suc`. Update TwfID if present. A
   `<pwpErrorCode>16</pwpErrorCode>` in a successful reply is tolerated (profile redirect,
   ignored).

### 5.7 State diagram

```
           ┌──────────────┐ TwfID preset by user
  start ──►│ HaveTwfID?   │──────────────────────────────────────────────┐
           └──────┬───────┘                                              │
                  │ no                                                   │
                  ▼                                                      │
           PreAuth (login_auth) ──► [RndImg=1] Captcha ──┐               │
                  │                                       │               │
                  ▼◄──────────────────────────────────────┘               │
           Password (login_psw)                                           │
            │ NextAuth=2|sms   │ NextAuth=7|token   │ NextAuth=0   │ -1/none + Result=1
            ▼                  ▼                    ▼              │
           SmsSend→SmsVerify  Totp                 CaFetch→CertLogin│
            │                  │                    │              │
            └──────────────────┴────────────────────┴──────────────┴──► Authorised(TwfID)
  any marker mismatch / transport error ──► Failed (terminal; caller restarts from start)
```

A user-supplied TwfID skips the whole machine (the server is trusted to accept it).

---

## 6. Configuration and resources (L6)

### 6.1 `/por/conf.csp`

Read `Conf` → `Mline` element. Attributes:

- `list`: `;`-separated `host:port` entries; empty entries skipped. Empty list → error.
- `enable`: the reference client effectively ignores it. Recommended: honour `enable="1"`
  **(inferred)**.

### 6.2 `/por/rclist.csp`

Root `Resource`. Children used:

**`Other`** — attribute `allocateVip`: value `0` ⇒ server does not allocate a virtual IP ⇒
TCP-proxy-only mode (§7.4); anything else/absent ⇒ L3 mode.

**`Rcs` / `Rc`** (required `Rcs`). Each `Rc`:

| Attribute | Semantics |
|---|---|
| `type` | `1` or `2` ⇒ network resource (processed); other values ignored. Exact meaning of 1 vs 2 is an **open question** |
| `proto` | `-1` all, `0` tcp, `1` udp, `2` icmp; other → unknown (protocol empty) |
| `host` | `;`-separated list; entry is `a.b.c.d~e.f.g.h` (inclusive IPv4 range), a single IP (optionally `ip:port`), or a domain/URL |
| `port` | `;`-separated list, same length as `host` (else skip the `Rc`); each entry `min~max` |
| others | e.g. `name`, `id` — not used |

Host entry normalisation (non-range): strip anything up to and including `//`; truncate at the
first `/`; remove all `*`; if it contains `:`, split host and port. If the remainder parses as an
IP it is an IP resource (and an explicit `:port` overrides the port range with that single
port); otherwise it is a domain resource (wildcard `*` removed, so `*.example.edu` becomes
suffix `.example.edu`). Domain resources are optional (configurable). Multiple `Rc` for the
same domain accumulate.

Resulting model (shared with aTrust): IP resource = {ip_min, ip_max, port_min, port_max,
protocol}; domain resource = {port_min, port_max, protocol} keyed by name. Match rule: protocol
equal or resource protocol `all`; port in range unless network is ICMP.

**`Dns`** (required):
- `data`: `;`-separated entries `<x>:<name>:<ipv4>`; exactly three `:`-parts else skipped.
  Produces static host overrides name → [ip…]. Meaning of `<x>` is an **open question**
  (likely a record id).
- `dnsserver`: `;`-separated DNS server IPs; first is primary; `0.0.0.0` means none. Missing
  attribute → error. No usable server while in L3 mode → error.

### 6.3 Line (multi-line) selection

If enabled, after authentication: fetch conf.csp, parse lines, probe each line in parallel:
3 rounds of TCP connect + modern-profile TLS handshake, 0.5 s apart, 1 s timeout each. Pick the
lowest mean latency among lines with 3/3 successes. If it differs from the current server:
switch the server, discard the TwfID and re-run authentication once against the new line (no
second probe). Failures in this step are non-fatal (keep current line). `client/easyconnect/line.go`

---

## 7. Tunnel layer (L7)

### 7.1 Token derivation (probe)

1. Open a new TCP connection to the server and complete a **modern**-profile TLS handshake.
2. Send the pipelined request of §3.1; read ≥ 1 byte (error if 0 bytes).
3. `S` = lowercase hex of the ServerHello `session_id` (32 bytes → 64 chars).
4. Token (48 bytes, ASCII) = first 31 chars of `S` ‖ `0x00` ‖ TwfID.
   This requires TwfID to be exactly 16 bytes; the reference client aborts otherwise. Whether
   other lengths occur is an **open question**.
5. Close the probe connection.

### 7.2 Virtual IP request (`L3IP` channel, opcode 0x00)

Request (64 bytes) on a fresh legacy-TLS `L3IP` channel:

```
 0       1       2       3
+-------+-------+-------+-------+
| 0x00  | 0x00  | 0x00  | 0x00  |  opcode = 0x00 (request IP), 3 zero bytes
+-------+-------+-------+-------+
|                               |
|        token (48 bytes)       |  bytes 4..51
|                               |
+-------+-------+-------+-------+
|      zero (8 bytes)           |  bytes 52..59
+-------+-------+-------+-------+
| 0xFF  | 0xFF  | 0xFF  | 0xFF  |  bytes 60..63: "no IP yet" (0xFFFFFFFF)
+-------+-------+-------+-------+
```

Reply (read up to 128 bytes, one read): byte 0 must be `0x00`; bytes 4..7 = assigned IPv4 in
network order. Other reply bytes: **open question**.

Constraints:
- Leave at least 1 s between the token probe (§7.1) and this request. A request made sooner
  can be rejected by the server (rate limiting, **inferred**).
- Keep this connection open until teardown. The data-channel handshakes of §7.3 succeed only
  while it is still open (the address lease appears to be bound to it, **inferred**). The
  reference client never reads or writes it again. Whether the server sends anything on it
  (e.g. heartbeats 0x0E/0x0F, IP change) is an **open question**; an implementation should
  drain and log it.
- A new IP request replaces (and closes) the previous pinned connection.

Define `ip_rev` = the 4 IP bytes reversed (little-endian order).

### 7.3 L3 data channels (`L3IP`, opcodes 0x05 / 0x06)

Two more `L3IP` channels, each with the same 64-byte layout as §7.2 but:

| Channel | Opcode byte 0 | bytes 60..63 | Success reply byte 0 | Direction afterwards |
|---|---|---|---|---|
| Send (client→server) | `0x05` | `ip_rev` | `0x02` | client writes IPv4 packets |
| Receive (server→client) | `0x06` | `ip_rev` | `0x01` | client reads IPv4 packets |

Reply is read with a single read of up to 1500 bytes; only byte 0 is interpreted.

Send-channel reply codes. The upper-case labels are the vendor client's names for these
commands as reported by the reference project; not all are independently verified:

| Byte 0 | Meaning | Client action |
|---|---|---|
| 0x02 | channel accepted | start sending |
| 0x05, 0x06, 0x07, 0x09 | RECONNECT_LATER: the server cannot serve the channel now (load, or a clash with another session) | drop the channel, wait, then try again; a new login may be needed first |
| 0x08 | SHUTDOWN: the server has ended this session | drop the channel; this TwfID is dead and only a new login helps |
| 0x0A | IPVALID | not expected here; fail |
| 0x0E, 0x0F | HEARTBEAT | not expected here; fail |
| other | — | fail |

Receive channel: only `0x01` is success; anything else is an error.

**Data framing**: none beyond TLS. Each IPv4 packet is sent as one TLS write (→ one
application_data record, packets ≤ 1400–1500 bytes). On receive the reference client assumes
**one TLS record = one IPv4 packet**, reading into a 1500-byte buffer. A robust implementation
should split on the IPv4 total-length field if a record ever carries several packets or a
fragment **(inferred)**. IPv6 is not carried. Local interface MTU 1400.

**Recovery** (client policy). A failed write means the send channel is lost and a failed read
means the receive channel is lost. Either is replaced by opening a new channel with the same
handshake. The reference client allows 5 replacements per direction over the whole session
before treating the error as fatal; an implementation may choose its own limit and back-off. A
SHUTDOWN reply to a replacement handshake ends the session (see the table above).

### 7.4 TCP-proxy channel (`TCPP`)

Used when `allocateVip="0"` (no virtual IP; L3 channels are not opened), or per-resource policy.
One legacy-TLS channel with session_id tag `TCPP` per proxied TCP connection. Destination must be
IPv4 and port 1–65535. Exchange (all ASCII, CRLF line endings, each message terminated by an
empty line):

```
C → S:  C01 HELLO\r\nCLIENT: WINDOWS/10.0\r\n\r\n
S → C:  C01 OK\r\n … \r\n\r\n
C → S:  C02 AUTH SESSION\r\nID: <TwfID>\r\n\r\n
S → C:  C02 OK\r\n … \r\n\r\n
C → S:  C03 CONNECT RESOURCE\r\nVER: 4\r\nDST: <dotted IPv4> <decimal port>\r\nTYPE: GENERAL\r\n\r\n
S → C:  C03 OK\r\n … \r\n\r\n
then: raw bidirectional TCP payload bytes until either side closes.
```

Rules: send one command, then read the reply byte-by-byte up to and including the first
`\r\n\r\n` (never over-read: payload may follow immediately). Reply header limit 4096 bytes.
Reply must start with `<code> OK\r\n`; otherwise error carrying the reply. Additional reply
header lines and failure reply formats are an **open question**. Note `C02` authenticates with
the TwfID, not the 48-byte token. `VER: 4` presumably means IPv4 **(inferred)**.

Teardown: close the TLS/TCP connection. No in-band close message.

---

## 8. Session orchestration (L8)

### 8.1 Setup sequence

```
Client                                         Server :443
  |-- (TLS modern) GET /por/login_auth.csp ------->|  TwfID₀, RSA key, CSRF, RndImg
  |-- [GET /por/rand_code.csp] ------------------->|  captcha image
  |-- POST /por/login_psw.csp -------------------->|  Result / NextAuth / TwfID₁
  |-- [2nd factor: sms | token | cert] ----------->|  TwfID₂
  |-- [GET /por/conf.csp → line probe → maybe re-login on best line]
  |== TLS modern probe: pipelined conf+rclist ====>|  ServerHello.session_id → token
  |-- [GET /por/rclist.csp] ---------------------->|  resources, DNS, allocateVip
  |   (wait until ≥ 1 s since probe)
  |== TLS legacy "L3IP": op 0x00 ================>|  reply 00 .. IP          (kept open)
  |== TLS legacy "L3IP": op 0x05 ================>|  reply 02               (send channel)
  |== TLS legacy "L3IP": op 0x06 ================>|  reply 01               (recv channel)
  |   ... IPv4 packets: one per TLS record each way ...
  |-- every 60 s: GET /por/update_session.csp --->|  <Auth><Message>success</Message><ErrorCode>1</ErrorCode>…
  |   teardown: close all connections (no logout call)
```

In TCP-only mode (`allocateVip="0"`) the three `L3IP` steps are skipped and each proxied TCP
flow opens its own `TCPP` channel (§7.4). Resource fetch is optional (configurable); without it
L3 mode is assumed.

### 8.2 Session keepalive

Every 60 s, GET `update_session.csp` (10 s timeout). Success iff status 200 and the XML
`Auth` root has `Message`=`success` and `ErrorCode`=`1` (reply also carries `TwfID`, ignored).
404 ⇒ server lacks the endpoint ⇒ stop the keepalive loop. Other failures are logged and the
loop continues. Servers with a short idle limit expire a session that is never refreshed this
way; the tunnel channels then break and their replacement handshakes get non-success replies.

There is no client-originated in-band keepalive on the tunnel channels.

### 8.3 Teardown

Cancel keepalive, close the pinned IP-request connection, the send and receive channels, any
`TCPP` channels, and idle HTTPS connections. No logout request is sent (a logout endpoint,
likely `/por/logout.csp`, is an **open question**).

### 8.4 Inputs (configuration surface)

Server host, port; username, password; optional TOTP secret; optional PKCS#12 file + password;
optional pre-existing TwfID; multi-line probe on/off; fetch resources on/off; include domain
resources on/off; captcha image output location / challenge handler.

---

## 9. Golden vectors

All are derived from the stated rule: worked out for this document from the formats above, on
inputs chosen for this document (not captured from a live server). GV-9 is the one exception
and says so.

**GV-1 Legacy ClientHello, `L3IP` channel** (random = 00 01 … 1F). Record 91 bytes:

```
16 03 01 00 56                      record: handshake, ver 0x0301 (inferred), len 86
01 00 00 52                         ClientHello, len 82
03 02                               client_version TLS 1.1
00 01 02 … 1e 1f                    random (32)
20                                  session_id len 32
4c 33 49 50 00 00 … 00              "L3IP" + 28 × 00
00 04 00 05 00 ff                   cipher_suites
01 00                               compression: null
00 05 00 0f 00 01 01                extensions: heartbeat, mode 1
```

Full hex:
`16 03 01 00 56 01 00 00 52 03 02 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f 10 11 12 13 14 15 16 17 18 19 1a 1b 1c 1d 1e 1f 20 4c 33 49 50 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 04 00 05 00 ff 01 00 00 05 00 0f 00 01 01`

For `TCPP`, replace `4c 33 49 50` with `54 43 50 50`.

**GV-2 Token.** ServerHello session_id = `a1b2c3d4e5f60718293a4b5c6d7e8f90` repeated twice;
TwfID = `0123456789abcdef`. Token (48 bytes):
`61 31 62 32 63 33 64 34 65 35 66 36 30 37 31 38 32 39 33 61 34 62 35 63 36 64 37 65 38 66 39 00 30 31 32 33 34 35 36 37 38 39 61 62 63 64 65 66`
(= ASCII `a1b2c3d4e5f60718293a4b5c6d7e8f9` ‖ 00 ‖ `0123456789abcdef`).

**GV-3 IP request** (token from GV-2), 64 bytes:
`00 00 00 00` ‖ token ‖ `00 00 00 00 00 00 00 00 ff ff ff ff`

**GV-4 Send-channel handshake**, assigned IP 10.8.0.42 → `ip_rev` = `2a 00 08 0a`:
`05 00 00 00` ‖ token ‖ `00 00 00 00 00 00 00 00 2a 00 08 0a`

**GV-5 Receive-channel handshake**: as GV-4 with first byte `06`.

**GV-6 IP reply** (shape): `00 ?? ?? ?? 0a 08 00 2a …` → IP 10.8.0.42.

**GV-7 Password form body** (svpn_name `alice`, CSRF `abc`, ciphertext hex abbreviated as
`deadbeef`, no captcha):
`mitm=&svpn_name=alice&svpn_password=deadbeef&svpn_rand_code=&svpn_req_randcode=abc`
RSA plaintext for password `pw` and CSRF `abc`: ASCII `pw_abc` = `70 77 5f 61 62 63`.

**GV-8 TCPP C03** for 192.0.2.7:8443 (68 bytes):
`C03 CONNECT RESOURCE\r\nVER: 4\r\nDST: 192.0.2.7 8443\r\nTYPE: GENERAL\r\n\r\n`

**GV-9 update_session success body.** This reply shape cannot be derived from a rule. It is
known only from a fixture in the reference project (`client/easyconnect/underlay_test.go`) and
is kept for that reason:
`<Auth><Message>success</Message><ErrorCode>1</ErrorCode></Auth>`

**GV-10 Resource list** (derived from §6.2, own inputs):

```
<Resource><Rcs>
  <Rc type="1" proto="0" host="portal.example.org" port="443~443" />
  <Rc type="1" proto="0" host="portal.example.org" port="8080~8080" />
</Rcs><Dns data="" dnsserver="192.0.2.53" /></Resource>
```

Expected: domain `portal.example.org` has two TCP rules (443–443 and 8080–8080), and a lookup
on either port matches. DNS server 192.0.2.53. L3 mode, because there is no `Other` element.

**GV-11 Host normalisation** (derived from §6.2): `https://*.lib.example.edu/path` →
domain `.lib.example.edu`; `10.1.2.3:8080` with port `1~65535` → IP 10.1.2.3, port
8080–8080; `10.0.0.0~10.255.255.255` → IP range.

**GV-12 Captcha answer** (from the challenge validation rule, `client/authchallenge/types.go`): empty or whitespace-only text
is rejected.

---

## 10. Open questions and server-version variance

1. **Token probe under TLS 1.3**: if a server negotiates TLS 1.3 on the HTTPS port, the
   ServerHello `session_id` is an echo of the client's, and the token would be meaningless.
   Should the probe pin TLS ≤ 1.2? (Reference client offers 1.3.) Resolve by capture against a
   current appliance.
2. **TwfID length**: token layout assumes exactly 16 bytes. Are longer IDs ever issued?
3. **Send-channel success code 0x02 vs table code 0x00 (SEND_IP)**: relationship unclear; the
   receive-side code table is unknown.
4. **Pinned IP-request channel**: what, if anything, does the server send on it (heartbeat
   0x0E/0x0F, IP change, shutdown)? Should the client answer heartbeats?
5. **L3 record ↔ packet mapping**: guaranteed one packet per record from the server, or can
   records coalesce/split?
6. **IP reply bytes 1–3 and 8–127**: unknown (netmask, gateway, DNS?).
7. **Bytes 52–59 of channel messages** (always zero here): purpose unknown.
8. **NextAuth codes** other than −1/0/2/7 (e.g. RADIUS challenge, hardware key, captcha-only)
   and their endpoints.
9. **`Rc type`** 1 vs 2 semantics; other types (web resources) ignored.
10. **`Dns data` first field** meaning.
11. **`Mline enable`** semantics (reference ignores it).
12. **Header sensitivity**: does any server check `User-Agent` or require
    `Content-Type` on POST? Do any endpoints redirect?
13. **Logout**: endpoint and whether it frees the server session/VIP faster.
14. **TCPP failure replies** and extra reply headers; meaning of `VER` and `TYPE` values
    beyond `4`/`GENERAL`; whether UDP has an equivalent.
15. **Legacy TLS variance**: servers that refuse RC4 or TLS 1.1 (newer EasyConnect builds may
    accept other suites under the same session_id tag). Reference evidence covers 7.6.x-era
    servers (`VPNVERSION` is logged but not branched on).
16. **Cert login renegotiation**: confirm the server requests the client certificate via
    renegotiation rather than in the initial handshake.
17. **SMS / welcome markers are localised Chinese strings**; English-locale servers may differ.
18. **Rate limits**: the "≥ 1 s between token and IP request" rule is empirical; other timing
    constraints unknown.
