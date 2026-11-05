# examples (`protocol-examples`)

Runnable dual-use demos that compose forged packets with the protocol crates and inject them
through `rawsock`. Not a protocol and not a layer; `publish = false`, no public API.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) and protocol guide [`../AGENTS.md`](../AGENTS.md) also apply.

## Examples

- `spoof_udp` — `Ip(Udp(..))` with a forged source IP, sent through rawsock's L3
  `NetworkSocket` (the `network` feature).
- `forge_arp` — a gratuitous ARP reply framed as `Ethernet(ArpPacket)`, sent on a named
  interface through rawsock's L2 `LinkSocket` (the `link` feature).

The crate enables rawsock's privileged `network` and `link` backends and each protocol crate's
`inject` feature. Composing works unprivileged; the send needs `CAP_NET_RAW` (each demo falls
back to printing the composed bytes). The socket backends are Linux-only; elsewhere the
examples build as stubs.

```text
cargo run -p protocol-examples --example spoof_udp
sudo -E cargo run -p protocol-examples --example forge_arp -- eth0
```

**Authorized testing only**: spoofing and ARP poisoning a network you do not own or have
permission to test is illegal.

## Conventions

`#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`. A new demo goes in `examples/` and is listed
in `src/lib.rs` and here.
