# application/sangfor/atrust

A client for the Sangfor aTrust VPN protocol, from the clean-room specification
[`../SPEC-atrust.md`](../SPEC-atrust.md). refcheck protocol name: none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — in progress: the policy model and the control-plane API

| Module | Specification | Stage |
|---|---|---|
| `policy` | §6 | functional: parsing and lookups, matching vectors V21 and V23 to V26 |
| `api` | §4 | functional for a password login: requests E1 to E3, E5, E10, E11, E22, E23 and their answers, matching vectors V6, V7 and V18 |
| `login` | §5 | not written: password login first |
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

## Rules

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
- `tests/support/` — a response built from its wire form, shared by both.

A fuzz target for `Policy::parse` is owed, with those of the crates below.

Run: `cargo test -p sangfor-atrust`.
