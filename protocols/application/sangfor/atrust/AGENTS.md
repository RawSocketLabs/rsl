# application/sangfor/atrust

A client for the Sangfor aTrust VPN protocol, from the clean-room specification
[`../SPEC-atrust.md`](../SPEC-atrust.md). refcheck protocol name: none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — in progress: the policy model only

| Module | Specification | Stage |
|---|---|---|
| `policy` | §6 | functional: parsing and lookups, matching vectors V21 and V23 to V26 |
| `api` | §4 | not written: request builders and response parsers over a `Portal` trait |
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

## Rules

- **Document order is meaning.** The first and the last matching rule are different answers
  (§6.5), so rules are stored and scanned in the order the document gives them.
  `Policy::ip_matches` is a double-ended iterator for that reason.
- **Missing or mistyped members yield nothing.** Only text that is not JSON is an error. A
  gateway's error envelope therefore parses as an empty policy; checking the envelope is the
  caller's job.
- **Names are compared normalised**: trimmed, lower-cased, one trailing dot removed. Rule
  names are normalised when parsed and queried names in each lookup, so callers pass names
  as they got them.
- **Where the specification permits a difference from the reference client, this crate takes
  it**, and `Policy::parse` documents each: an application with no id is dropped; white
  space around a host is removed before the host is classified; a pin's host is normalised,
  and a suffix gets no pins; a port number written with a sign is malformed; a node with no
  address is dropped. Do not add to this list without the specification saying an
  implementation may.
- **A hostile document costs memory in proportion to its size.** What an application's rules
  share (`Target`) and what an entry's pins share (the host name) are behind `Arc`, never
  copied per rule: one long id beside thousands of addresses would otherwise multiply.
- **Numbers too large to hold saturate.** The protocol does not range-check port numbers
  (§6.2), so an all-digit number past `u64` is treated as past every bound, not as malformed.
- **The rule types are built by `Policy::parse` only.** They are `#[non_exhaustive]` with
  public fields and no constructors; add constructors when something other than a gateway's
  document has to supply a policy.
- **Rotation among several pinned addresses is the caller's.** `Policy::pinned` yields them
  in order and keeps no state.
- **A node's own address is looked up like any other** (§6.4). This crate installs no
  routes, so it has nothing to take out; a caller that fears a loop can decline such a
  destination.

## Testing

- **`contract`** — `tests/contract.rs`: the specification's vectors V21 and V23 to V26, and
  the rules §6 states without a vector.
- **`adversarial`** — `tests/adversarial.rs`: text that is not JSON, JSON with no policy in
  it, members of the wrong type, nesting too deep to walk, numbers too large to hold, and
  that rules share what they could otherwise be made to copy.

A fuzz target for `Policy::parse` is owed, with those of the crates below.

Run: `cargo test -p sangfor-atrust`.
