# rawsock — roadmap

Status and plan of record. This crate is extracted from the proven asyio `rawsock` (rev 3)
and re-homed as a bnb-independent repo.

## Built — the core

- [x] **`RawIo` / `Layer` / `OpenError`** — the dual-use sink trait + layer taxonomy.
- [x] **`compose`** — `Protocol`/`ProtocolExt`/`Context`/`Pseudo`, `.payload()` nesting,
      compliant `encode` vs verbatim `encode_raw`, cross-layer checksums, `internet_checksum`
      (RFC 1071).
- [x] **`Loopback`** — in-memory `RawIo` for tests/dry-runs (any OS, no privilege).
- [x] **`capabilities()`** — host+process layer-openability probe.
- [x] **`transport` (L4)** — unprivileged UDP via `rustix`.
- [x] rustix-only core, CI-testable everywhere (now `#![deny(unsafe_code)]`; see `link` below).

## The privileged backends

- [x] **`network` (L3)** — **done** (`network` feature): `NetworkSocket`, raw IPv4 via
      `SOCK_RAW`/`IPPROTO_RAW` (`IP_HDRINCL` free), `send(&impl Protocol)` + `RawIo`. Triggered by
      the `protocols` `ip` layer (the first crate to hand-build IP headers). Tested via a
      capability-consistency check (runs in CI) + a `CAP_NET_RAW`-gated loopback send.
- [x] **`link` (L2)** — **done** (`link` feature): `LinkSocket`, raw Ethernet via `AF_PACKET`,
      bound to an interface by name. rustix's `netdevice::name_to_index` covers the interface
      lookup, but rustix 1.1.4 (the latest) has **no safe link-layer socket address** — so
      binding needs **one** `unsafe` call to build a `sockaddr_ll` via `SocketAddrAny::read`.
      Per the plan below, the crate went `forbid(unsafe_code)` → `deny` with that single
      `#[allow]` confined to `linux/link.rs`; **no `libc`** (rustix-only). Capability-consistency
      test (CI) + a `CAP_NET_RAW`-gated loopback send.
- [x] **`spoof_udp` / `forge_arp` examples** — **done**, in the `protocol-examples` crate (`protocols/examples`):
      `spoof_udp` composes an `Ip(Udp(..))` with a forged source IP and injects via `NetworkSocket`
      (L3); `forge_arp` composes an `Ethernet(Arp reply)` cache-poisoning frame and injects via
      `LinkSocket` (L2). Both fall back to compose-only when unprivileged.
- [ ] **Concrete compose tests** with the real IP/UDP `Protocol` impls (the current
      `tests/compose.rs` uses an in-test toy stack).

## Open decisions

- [x] **Dependency in `protocols`** — done: the IP/ICMP/UDP/TCP/Ethernet/ARP crates depend
      on `rawsock` (workspace path dep) behind their optional `inject` feature.
- [ ] **rustix version** — track 1.x; on each upgrade, re-check for a safe link-layer socket
      address that would let `link_addr` drop its `unsafe`.
- [ ] **macOS / Windows backends** — additive behind `RawIo`; `OpenError::DriverMissing`
      reserved for the Windows (WinDivert/Npcap) path.
