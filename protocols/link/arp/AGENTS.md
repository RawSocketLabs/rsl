# link/arp

ARP (RFC 826) packet codec on `bnb`, IPv4-over-Ethernet. refcheck protocol name: **`arp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — packet codec + rawsock injection

Decode/encode of the 28-byte IPv4-over-Ethernet ARP packet, plus (the `inject` feature) a
`rawsock::Protocol` impl so it can be framed and injected.

## Architecture

- **`Operation`** — `#[derive(BitEnum)]` `#[bit_enum(u16, bytes = be)]`: `Request` (1), `Reply`
  (2), `#[catch_all] Other(u16)` (dual-use — unknown ops preserved).
- **`ArpPacket`** — `#[bin(big)]`: `htype`/`hlen`/`plen`/`oper`, `ptype` (the `ethertype`
  crate's `EtherType`), `sha`/`tha` (`[u8; 6]` MACs, bnb's native byte-array fields), and
  `spa`/`tpa` (`Ipv4Addr` — bnb's native `std::net` address codec). Two consumers of shared
  types in one 28-byte struct. `request(sha, spa, tpa)` / `reply(sha, spa, tha, tpa)`
  constructors set the fixed Ethernet/IPv4 fields (`htype 1`, `ptype IPv4`, `hlen 6`, `plen 4`).

## Injection — the `inject` feature (`src/inject.rs`)

ARP is a **leaf** `Protocol` (a complete message, no payload): `impl Protocol for ArpPacket`,
`protocol_id` → `0x0806` (its EtherType, presented to an enclosing Ethernet frame), `layer` →
`Network`. It has **no derived fields** (no checksum, no length), so `encode == encode_raw ==
to_bytes` — everything verbatim. rawsock is a git dep behind the feature (`default-features =
false` → no `rustix`). `Ethernet(ArpPacket)` composes a complete L2 frame.

## Dual-use

Every field is stored verbatim — the parser rejects nothing representable, so a packet with an
unusual `oper` or mismatched `hlen`/`plen` round-trips unchanged.

## Testing

- `unit` (inline): the `request`/`reply` constructors' fixed fields.
- `tests/integration.rs`: a golden request (field-correct, byte-identical round-trip); an
  unknown operation preserved as `Other`.
- `tests/adversarial.rs`: truncated packet, arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`, dev-dep `ethernet`): `protocol_id`/`layer`; encode is
  verbatim with no derived fields; `Ethernet(ArpPacket)` sets the EtherType and sends out an L2
  `Loopback` sink.
- Example: `who_has` (`--features inject`: an ARP request broadcast, framed in Ethernet).

Run: `cargo test -p arp` (add `--features inject` for the rawsock layer).

## Scope / follow-ups

IPv4-over-Ethernet only (fixed `[u8; 6]` / `Ipv4Addr` addresses). The general variable-length
form (arbitrary `hlen`/`plen`, e.g. non-Ethernet hardware) is a later refinement.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
