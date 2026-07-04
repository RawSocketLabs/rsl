# network/ip

IPv4 (RFC 791) header codec on `bnb`. refcheck protocol name: **`ip`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec + rawsock injection

Decode/encode of the 20-byte fixed IPv4 header plus raw options, plus (the `inject` feature)
the `rawsock::Protocol` layer that wraps an L4 payload into a full datagram. The composition
keystone: the IP layer is what supplies the pseudo-header a UDP/TCP checksum needs.

## Architecture

- **`VersionIhl`** — byte 0, `#[bitfield(u8)]`: `version: u4`, `ihl: u4`.
- **`FlagsFragment`** — bytes 6–7, `#[bitfield(u16)]`: `reserved`/`dont_fragment`/`more_fragments`
  bools + `fragment_offset: u13`.
- **`Ipv4Header`** — `#[bin(big)]`: the two bitfields, `dscp_ecn`, `total_length`,
  `identification`, `ttl`, `protocol`, `header_checksum`, `src`/`dst` (`Ipv4Addr` fields — bnb
  encodes `std::net` addresses natively), and `options: Vec<u8>` sized by `ihl` (`(ihl - 5) * 4`,
  `saturating_sub` — a malformed `ihl < 5` reads zero option bytes, never panics).
  `Ipv4Header::datagram(src, dst, protocol, payload_len)` builds a standard header;
  `header_len()` = `IHL * 4`.

## Injection — the `inject` feature (`src/inject.rs`)

`Ip<P>` wraps an `Ipv4Header` + payload (`protocol_id` → `0x0800`, `layer` → `Network`).
`encode` (compliant) is the keystone: it hands the payload the IPv4 **pseudo-header**
(src/dst/protocol) in a child `Context` so the L4 layer can checksum, then fills
`total_length` + `protocol` from the payload and computes the IPv4 **header checksum** (on
rawsock's `internet_checksum`). `encode_raw` emits everything verbatim (dual-use). rawsock is a
git dep behind the feature (`default-features = false` → the `compose` trait + `Loopback`, no
`rustix`). Compose a real packet: `Ip::new(Ipv4Header::datagram(..), Udp::new(..)).encode()`.

## Dual-use

`total_length` and `header_checksum` are stored **verbatim** — decode never recomputes,
verifies, or rejects, so a forged length or checksum survives a round-trip.

## Testing

- `unit` (inline): `datagram` field defaults.
- `tests/integration.rs`: a golden header (field-correct, byte-identical round-trip); a forged
  `total_length` preserved.
- `tests/adversarial.rs`: `ihl < 5` (no underflow), truncated, options past the buffer,
  arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`, dev-dep `udp`): the full `Ip(Udp(..))` stack fills
  length/protocol and **both** checksums verify to 0 (RFC 1071); raw preserves forged fields;
  the datagram goes out a `Loopback` sink.
- Example: `inject_stack` (`--features inject`: compose a full IPv4+UDP datagram, verify, send).

Run: `cargo test -p ip` (add `--features inject` for the rawsock layer).

## Scope / follow-ups

IPv4 only (IPv6 later). No fragmentation/reassembly logic (the fields are exposed; policy is
the caller's). `network/icmp` and the L2 `link/*` layers are the next network-stack pieces.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
