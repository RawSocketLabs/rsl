# application/sangfor/atrust

A client for the Sangfor aTrust VPN protocol, from the clean-room specification
[`../SPEC-atrust.md`](../SPEC-atrust.md). refcheck protocol name: none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — in progress: a password login works against a scripted gateway

| Module | Specification | Stage |
|---|---|---|
| `policy` | §6 | functional: parsing and lookups, matching vectors V21 and V23 to V26 |
| `api` | §4 | functional for a password login: requests E1 to E3, E5, E10, E11, E22, E23 and their answers, matching vectors V6, V7 and V18 |
| `login` | §5 | functional for a password login with no second factor: the sequence of §5.2, the anti-MITM checks of §5.3 (vectors V1 to V6), next-step resolution (V19); captcha, one-time codes and external logins are reported as unsupported |
| `tcp` | §9 | not written: a sans-I/O handshake and frame core with a blocking driver |
| `net` | §2 | not written: the real socket, TLS and HTTP implementations, behind a feature |

The first milestone is password login and one TCP stream through a node. Nothing here has
been checked against a real gateway.

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

## Rules

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
  script (`Script`, the test double for `Portal`), a counting stand-in for randomness, and
  the fixtures of the specification's vectors.

`login.rs` in `contract` has the sequence and what each request carries from the answers
before it, V28, the anti-MITM checks in order with V1, V3, V4 and V6, and the §5.5 table with
V19; in `adversarial` it has gateways that refuse, stall or break off at each step.

A fuzz target for `Policy::parse` is owed, with those of the crates below.

Run: `cargo test -p sangfor-atrust`.
