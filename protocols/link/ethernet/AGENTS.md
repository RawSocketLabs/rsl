# link/ethernet

Ethernet II (IEEE 802.3) frame-**header** codec on `bnb`. refcheck protocol name: **`ethernet`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec + rawsock injection

Decode/encode of the 14-byte Ethernet II frame header, plus (the `inject` feature) a
`rawsock::Protocol` layer that frames an L3 payload for L2 injection — the **top of the stack**.

## Architecture

- **`EthernetHeader`** — `#[bin(big)]`: `dst`/`src` (`[u8; 6]` MAC, encoded via bnb's native
  byte-array field support), `ethertype` (the `ethertype` crate's `EtherType` `BitEnum`). 14
  bytes; `HEADER_LEN` const, `BROADCAST` MAC const. No FCS — the NIC computes + appends the
  4-byte frame check sequence on transmit (`AF_PACKET`), so it isn't part of the codec.

## Injection — the `inject` feature (`src/inject.rs`)

`Ethernet<P>` wraps an `EthernetHeader` + payload at `Layer::Link`, `protocol_id` → `None` (L2
is the outermost frame — nothing demuxes it upward). `encode` sets the frame's `EtherType` from
the payload's demux id (IPv4 → `0x0800`, ARP → `0x0806`); `encode_raw` emits it verbatim
(dual-use forging). This closes the ladder: `Ethernet(Ip(Udp/Tcp/Icmp(..)))` composes a
complete, on-the-wire frame. rawsock is a git dep behind the feature (`default-features = false`
→ no `rustix`).

## Dual-use

The `EtherType` `#[catch_all] Custom` preserves an unknown protocol id; `encode_raw` never
rewrites the frame, so a forged EtherType survives.

## Testing

- `unit` (inline): header round-trip with the `EtherType` enum.
- `tests/integration.rs`: a golden frame header (field-correct, byte-identical round-trip); an
  unknown EtherType preserved as `Custom`.
- `tests/adversarial.rs`: truncated header, arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`, dev-deps `ip`/`icmp`): `protocol_id`/`layer`;
  compliant encode sets the EtherType from the payload; raw preserves a forged one; the full
  `Ethernet(Ip(Icmp(..)))` stack frames a pingable packet with **both** nested checksums
  verifying; the frame goes out an L2 `Loopback` sink.
- Example: `frame_stack` (`--features inject`: compose the whole `Ethernet(Ip(Icmp))` stack).

Run: `cargo test -p ethernet` (add `--features inject` for the rawsock layer).

## Scope / follow-ups

Ethernet II only (no 802.1Q VLAN tag / 802.3 LLC-SNAP length framing). `link/arp` (the other
`ethertype` consumer) and rawsock's `link` (`AF_PACKET`) L2 backend are the remaining L2 pieces.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
