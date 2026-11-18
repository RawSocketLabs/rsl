# application/sangfor/atrust

A client for the Sangfor aTrust VPN protocol, from the clean-room specification
[`../SPEC-atrust.md`](../SPEC-atrust.md). refcheck protocol name: none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — the first milestone is built and untried against a real gateway

| Module | Specification | Stage |
|---|---|---|
| `policy` | §6 | functional: parsing and lookups, matching vectors V21 and V23 to V26 |
| `api` | §4 | functional for a password login: requests E1 to E3, E5, E10, E11, E22, E23 and their answers, matching vectors V6, V7 and V18 |
| `login` | §5 | functional for a password login with no second factor: the sequence of §5.2, the anti-MITM checks of §5.3 (vectors V1 to V6), next-step resolution (V19); captcha, one-time codes and external logins are reported as unsupported |
| `tcp` | §9, §6.6 | functional: routing, the signed request, the handshake and both data phases, matching vectors V14 to V17, V26 and V27 |
| `net` | §2 | functional behind the `net` feature: `HttpsPortal` and `dial` over `std::net`, `tls` and `http1`, checked on loopback against `openssl s_server` |

The first milestone is password login and one TCP stream through a node. The protocol works
against scripted peers, and the network layer against a local TLS server; `examples/tunnel.rs`
puts them together. **Nothing here has been checked against a real gateway**, and a node's TLS
has never been observed: `tls` offers one cipher suite, and the first real run may stop there.

## Architecture

One crate, one module per part of the protocol; the pure primitives and the wire codecs are
the sibling crates `sangfor-atrust-sig` and `sangfor-atrust-wire`.

### `policy`

No I/O and no name resolution. One type per file:

- `document.rs` — `Policy`: the rules in document order, the pins, DNS servers and node
  groups, and every lookup.
- `parse.rs` — `Policy::parse`: a walk over the JSON value, because key spelling is not
  fixed and serde's derive cannot match keys without regard to case.
- `ip_rule.rs`, `domain_rule.rs` — the two kinds of rule and what each covers.
- `ports.rs`, `protocol.rs`, `target.rs`, `node_group.rs`, `name_route.rs` — their parts.

Lookups are linear scans in document order. The specification suggests an interval tree;
nothing has measured a need for one.

### `api`

Pure: no I/O, no clock, no randomness. A `Call` holds what is particular to one request of
the endpoint catalogue; `Call::request` completes it with a `Context` (the session's values
for this request) into an `http1::Request`. Each answer type has a `parse` that takes the
`http1::Response` and applies that endpoint's own rules for status and code.

- `call.rs` — `Call`, one constructor per endpoint, and the common query and headers.
- `context.rs` — `Context`: gateway, CSRF token, trace id, sign key, cookies.
- `gateway.rs` — `Gateway`: host and port, and the "base host" three things derive from.
- `portal.rs` — `Portal`, the crate's one control-plane I/O seam, and its `Reply`.
- `envelope.rs` — `Envelope`: code, message, raw payload; the status and code rules.
- `members.rs` — how members are read: null as absent, objects only where objects belong,
  lists bounded.
- `auth_config.rs`, `anti_mitm.rs`, `psw_reply.rs`, `next_step.rs`, `online_info.rs`,
  `manifest.rs` — the answers.

The login sequence (which request follows which, the cookie jar, the anti-MITM checks) is
the `login` module's, not this one's.

### `login`

Plain blocking code over `api::Portal`: each request is sent and answered before the next is
built. Time and randomness are arguments; nothing reads a clock or an OS generator.

- `session.rs` — `Session`: the cookie jar, the CSRF token, the key material and the sign
  key of the latest `authConfig` answer. `send` is the one place a request goes out and its
  cookies come in.
- `sequence.rs` — `Session::login`, and the three stretches it is made of: the anti-MITM
  checks, the password with its environment report, and the secondary chain.
- `step.rs` — `Step::resolve`: the next-step table of §5.5.
- `anti_mitm.rs`, `outcome.rs`, `options.rs`, `credentials.rs`, `device.rs`, `error.rs` — the
  values that go in and come out.

A login returns a `Login` (session identifier, user name, policy, zero-RTT flag, anti-MITM
outcome) and leaves the `Session` holding what later control-plane requests need.

### `tcp`

The protocol parts do no I/O; one type drives them over a blocking transport.

- `destination.rs` — `Destination::route`: §6.6 steps 1 to 5 on a `Policy`, yielding what
  goes on the wire and into the request. Resolving a name the policy does not pin is a
  closure the caller supplies.
- `identity.rs` — `Identity`: session identifier, user name, device id, connection id and
  the tunnel sign key. Fixed at client setup but for the session identifier.
- `request.rs` — the signed JSON document of §9.2, serialised from a struct so that the
  bytes signed are the bytes sent.
- `handshake.rs` — `Handshake`: the first flight, and the node's answer read step by step
  from a growing buffer with the `sangfor-atrust-wire` types. `Mode` and `Progress`.
- `frames.rs` — `frame` and `FrameReader`: the framed data phase, in whatever pieces.
- `stream.rs` — `Stream<T: Transport>`: `Read + Write` over a connection to a node, and
  `close_write`. `Transport` is `Read + Write` plus stopping the write side, which a TLS
  connection can do and a plain byte stream cannot say.

### `net` (feature `net`)

What a client needs to reach a real gateway, kept apart so that the protocol builds and is
tested without it.

- `portal.rs` — `HttpsPortal`: the `api::Portal` over a TCP connection, a TLS handshake and
  one HTTP exchange per request, returning the certificate chain it saw.
- `node.rs` — `dial`: the TLS connection to a node, and `tcp::Transport` for it
  (`close_write` is TLS `close_notify`).
- `socket.rs` — the TCP connect with a time limit.

## Rules

- **`net` verifies no certificate** (§2.1): gateways and nodes present self-signed ones. On
  the control plane the protocol's anti-MITM checks stand in, and a login reports them; a
  node connection has no such check. Do not add verification as a default: the reference
  and the vendor clients have none, and a gateway's certificate would fail it.
- **`net` never makes its own randomness.** `HttpsPortal` and `dial` take a
  `crypto::RandomSource`; a real client passes `os_random::OsRandom`.
- **Time limits in `net` are per operation**: the connect, and each read or write. A whole
  exchange is not bounded, nor is name resolution, and the public docs say so. A deadline
  transport is owed. The limit stays on a node's socket after the tunnel opens; a caller
  whose stream may sit idle lifts it (the example does).
- **`dial`'s stream borrows the node address.** `tls::ClientConfig` ties the server name to
  the connection's lifetime, though nothing is read from it after the handshake. Until `tls`
  drops that tie, a caller keeps the address string alive beside the stream.
- **The routine CI gate does not build `net`.** It builds default features only, so a change
  in `tls` or `http1` can break this module unseen. Until the gate has a configuration with
  the feature, run `cargo clippy -p sangfor-atrust --features net --all-targets` and the
  interop tests by hand when touching those crates.
- **One connection per control-plane request.** The session is in the cookies (§2.2), so
  nothing depends on reuse; it costs a TLS handshake each time.
- **Nothing is sent after the first flight until the whole answer is in**, in either mode
  (§9.1). Whether a gateway takes data early in zero-RTT mode is an open question, so it is
  not tried.
- **The verdict's first two bytes are judged before its JSON is waited for.** On a TCP
  tunnel a refusing node sends `53` and a status and nothing more (§9.1), unlike on a
  session-authentication connection (§7.2), where the JSON is always read. `AuthReply`
  decodes both; here its start is looked at first.
- **A verdict is a JSON object whose `code`, when present, is the integer zero.** No JSON at
  all, a bare `null`, and a `code` of null or of another type all fail (§9.1). The message
  is only for the error text.
- **`Mode::reusable` needs both the gateway's zero-RTT and the node's flag** (§9.3). Only
  then does this end finish with a close frame; otherwise with the transport's notice.
- **A framed stream ends cleanly only between frames**: at a close frame, or when the
  connection ends with no frame begun. An end inside a header, inside a frame's data, or
  right after a header that promised data is a truncation.
- **A broken frame breaks the stream for good**, and a close frame keeps it closed: the
  reader remembers both. Data read before a bad frame is delivered before the break is
  reported, so the outcome does not depend on how the bytes were cut into reads.
- **A frame that went out in part ends the writing half.** A framed write or a close frame
  that fails may have left half a frame with the node; anything sent after it would be read
  as that frame's data. `Stream` remembers and refuses every later write and half-close. A
  half-close counts as done only once its notice went out.
- **Where a name's address comes from is this client's policy**, and differs from the
  reference in two ways (§6.6 step 4): the policy's pins come before the caller's resolver,
  where the reference asks the client's own table first; and the first pin is always used,
  where the reference rotates among several. `Destination::route` says so.
- **The request's process values are the reference client's**, by destination port (§9.2).
  They are cosmetic; the fingerprint must match the path.
- **Deadlines are the transport's.** The reference gives a node 18 s to answer the handshake;
  `Stream::open` blocks for as long as its transport does.
- **The anti-MITM outcome is advisory unless the caller says otherwise** (§5.3): the vendor's
  clients report a failed check and carry on, so `Options::strict_anti_mitm` is off by
  default, the password goes to a gateway that failed its checks, and `Login::anti_mitm`
  says what was found. The docs of `Session::login`, `Options` and `AntiMitm` say this where
  a caller will read it; keep them saying it. A second-stage exchange that fails to complete
  is a failed check, not a failed login.
- **A strict failure sends nothing more and keeps nothing**: no request follows the failed
  check, and the cookie jar goes back to what it was before the login, so a cookie planted
  by whoever failed the checks cannot ride into the next attempt.
- **Strict stops on a failed check, not on a missing one.** An answer with no anti-MITM
  object is `Absent`, and one that does not enable the certificate check can be `Passed` on
  the challenge and signature alone, which a peer in the middle can produce. Both are
  outcomes for the caller to judge.
- **The sign key follows every `authConfig` answer**, whatever the checks found, and exists
  even when the answer has no anti-MITM object (§3.3).
- **The session identifier is read last, and is the `sid` the gateway's root would get**
  (§5.2): any answer may replace it, a `sid` scoped to a narrower path is not it, and a
  login that ends without one, or with an empty one, has failed.
- **What this client cannot do is an error that names it**: `CaptchaRequired`,
  `UnsupportedStep`. Nothing is skipped or guessed to get past a factor.
- **A refusing policy answer is an error.** The reference does not look at that answer's
  code (§5.2) and would read the refusal as an empty policy.
- **Secrets are never printed.** `Credentials`, `Session` and `Login` have hand-written
  `Debug`s that leave out the password, the cookies and the session identifier, which is all
  a tunnel needs to act as the user.
- **`Step::resolve` follows §5.5 to the letter**, including that with no service named an
  entry with no type is preferred to the first entry. Whether a gateway sends such a list
  is a question for the specification's owner.
- **`api` builds and parses; it never sends.** Everything that touches the network goes
  through `Portal`, so the whole login can be tested against scripted answers, and a
  different driver (sans-I/O, async) would replace only the sequence above it.
- **Each answer's `parse` owns its endpoint's rules** (§4.3 notes, §5.2): E1, E2 and E3 must
  be status 200, and only on E2 do 401 and 403 mean a lost session; the others do not look
  at the status. `PswReply` is read whatever its code, because a refusal may be a demand for
  a captcha; `AntiMitmReply` leaves its code to the check that judges it.
- **Read the envelope before the payload.** An error answer's `data` may be null or of
  another shape, and that must not hide its code and message.
- **Only a JSON object is read as an object.** serde's derived readers accept an array for a
  struct, member by position; every object here, nested ones included, goes through
  `members::from_object`, `members::object` or `members::objects`. A new answer type must
  use them for each member that is an object or a list of objects.
- **A null member is an absent member; a member of the wrong type is an error.** Strings are
  stored as received, empty included (§5.2); nothing is trimmed or defaulted to a guess.
  Member names are matched exactly.
- **Lists are bounded** at `members::MAX_ENTRIES`: an answer of many empty entries would
  otherwise cost about a hundred times its size in memory.
- **The anti-MITM object keeps its text.** Its signature covers the object as received,
  number spellings included (§2.5), so `AntiMitmData::raw` is the text and is never
  re-serialised.
- **Bodies that are signed are serialised from structs**, whose member order is fixed in the
  source. A `serde_json::Value` would follow whatever map the workspace's features select.
- **Nothing reaches a header unchecked.** String headers go through `Request::header`, which
  refuses a line break; cookie bytes, which bypass it, get the same check in `Call::request`,
  because a cookie restored from storage was never a checked response header.
- **The second-stage anti-MITM body is a byte template** (§4.3): nonce and ticket go in
  verbatim, unescaped. Whether a ticket can hold a quote is a question for a real gateway.
- **One source for each session value.** The device id is an argument of the calls that send
  it, not also a field of `Context`.
- **No `Accept-Encoding`** (§2.2): answers are read as sent.

## Testing

One test binary per role, one file per module inside it:

- **`contract`** — `tests/contract/`: `policy.rs` has vectors V21 and V23 to V26 and the
  rules §6 states without a vector; `api.rs` has every request's method, path, query,
  headers and content type, the bodies of V6 and V7 byte for byte, the encodings of V8, the
  status and code rules of each answer, and the fields of V18, V19 and V28 as they arrive.
- **`adversarial`** — `tests/adversarial/`: `policy.rs` has text that is not JSON, members of
  the wrong type, nesting too deep, numbers too large, and sharing instead of copying;
  `api.rs` has session values and cookies that try to inject a header, answers that are not
  objects at any level, null and mistyped members, lists past the bound, and error answers
  with any payload.
- `tests/support/` — a response built from its wire form, a gateway that answers from a
  script (`Script`, the test double for `Portal`), a node that does (`Node`, the double for
  `tcp::Transport`, which can interrupt reads and fail writes and closes), a counting
  stand-in for randomness, and the fixtures of the specification's vectors.

`tests/interop.rs` (feature `net`, ignored by default) runs `HttpsPortal` and `dial` against
`openssl s_server` on loopback: `cargo test -p sangfor-atrust --features net --test interop --
--ignored`. It proves the layers fit together (the chain returned is the server's, limits
are applied, data sent after this end's `close_notify` still arrives), not that a gateway
agrees. The pure parts of `net`, which names get offered in the handshake and how a node
address splits, have inline `unit` tests that run with the feature.

`tcp.rs` in `contract` has V26's routes, V15's request byte for byte, the answers of V16 and
V27 however they are cut, V17's frames, and both kinds of stream over a scripted node; in
`adversarial` it has verdicts that are not verdicts, nodes that hang up, and generated input.

`login.rs` in `contract` has the sequence and what each request carries from the answers
before it, V28, the anti-MITM checks in order with V1, V3, V4 and V6, and the §5.5 table with
V19; in `adversarial` it has gateways that refuse, stall or break off at each step.

The fuzz targets are in [`../fuzz`](../fuzz/README.md): `api` for the answer parsers,
`policy` for `Policy::parse` and the lookups, `tunnel` for `tcp::Handshake` and
`tcp::FrameReader` through a `tcp::Stream`, and `login` for the sequence against a gateway that
answers anything. Routine CI does not build them: after changing a public item here, build
them as that guide says.

Run: `cargo test -p sangfor-atrust`, and `cargo test -p sangfor-atrust --features net` to
build the network layer too.
