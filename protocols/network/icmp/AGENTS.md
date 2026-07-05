# network/icmp

ICMP (RFC 792) message-**header** codec on `bnb`. refcheck protocol name: **`icmp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec + rawsock injection

Decode/encode of the 8-byte ICMP header, plus (the `inject` feature) a `rawsock::Protocol`
layer that wraps the message data and computes the **self-contained** checksum.

## Architecture

- **`IcmpHeader`** — `#[bin(big)]`: `icmp_type`, `code`, `checksum`, `rest_of_header` (`u32`,
  type-specific). Type consts `ECHO_REQUEST` (8) / `ECHO_REPLY` (0) / `DEST_UNREACHABLE` (3) /
  `TIME_EXCEEDED` (11); `echo_request(id, seq)` / `echo_reply(id, seq)` constructors pack the
  identifier + sequence into `rest_of_header`; `identifier()` / `sequence()` read them back.
- **`IcmpMessage`** (`message.rs`) — a typed **view** over `(IcmpHeader, data)`: `EchoRequest`/
  `EchoReply { id, seq, data }`, `DestinationUnreachable`/`TimeExceeded { code, data }` (the
  error's embedded datagram), and `Other` (raw, preserved). `header.message(data)` /
  `IcmpMessage::parse` classify; `.header()` / `.data()` build back. A lens like `tcp`'s
  `TcpOption` — a plain parse/build, not a `#[bin]` union (sidesteps bnb's ctx-dispatch gap).

## Injection — the `inject` feature (`src/inject.rs`)

`Icmp<P>` wraps an `IcmpHeader` + data payload (`protocol_id` → 1, `layer` → `Transport` — it
occupies the same IP-payload slot as UDP/TCP). The distinguishing trait: the ICMP checksum is
**self-contained** — over the header + data, with **no IP pseudo-header** — so `encode`
computes it **unconditionally** (the enclosing `Context` is ignored), unlike UDP/TCP. `encode`
(compliant) fills it via rawsock's `internet_checksum`; `encode_raw` emits `.header.checksum`
verbatim (dual-use). rawsock is a git dep behind the feature (`default-features = false` → no
`rustix`). Compose a ping: `Ip::new(Ipv4Header::datagram(.., 1, ..), Icmp::new(echo, data))`.

## Dual-use

`checksum` is stored **verbatim** — decode never recomputes, verifies, or rejects it, so a
forged checksum survives a round-trip.

## Testing

- `unit` (inline): the echo constructors' id/seq packing.
- `tests/integration.rs`: golden headers (echo request; time exceeded; a forged checksum) —
  field-correct and byte-identical round-trips.
- `tests/adversarial.rs`: truncated header, arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`, dev-dep `ip`): `protocol_id`/`layer`; the checksum is
  self-contained and **verifies to 0** (RFC 1071); raw preserves a forged checksum; the full
  `Ip(Icmp(..))` stack has **both** checksums verify; the message goes out a `Loopback` sink.
- Example: `ping` (`--features inject`: compose a full IPv4 + ICMP echo request, verify, send).

Run: `cargo test -p icmp` (add `--features inject` for the rawsock layer).

## Scope / follow-ups

A flat header codec (type/code/checksum/rest) — deliberately **not** a tag-dispatched union
over `icmp_type`, which would hit bnb's ctx-`BitEncode` gap (deferred to bnb's freeze review).
Typed message views (Echo / Destination Unreachable / Time Exceeded) are a later refinement.
ICMPv6 (RFC 4443, which *does* use a pseudo-header) is separate.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
