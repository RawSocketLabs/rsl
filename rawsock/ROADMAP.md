# rawsock — roadmap

Status and plan of record. This crate is extracted from the proven asyio `rawsock` (rev 3)
and re-homed as a bnb-independent repo.

## Built — the unprivileged core

- [x] **`RawIo` / `Layer` / `OpenError`** — the dual-use sink trait + layer taxonomy.
- [x] **`compose`** — `Protocol`/`ProtocolExt`/`Context`/`Pseudo`, `.payload()` nesting,
      compliant `encode` vs verbatim `encode_raw`, cross-layer checksums, `internet_checksum`
      (RFC 1071).
- [x] **`Loopback`** — in-memory `RawIo` for tests/dry-runs (any OS, no privilege).
- [x] **`capabilities()`** — host+process layer-openability probe.
- [x] **`transport` (L4)** — unprivileged UDP via `rustix`.
- [x] 100% safe core (`#![forbid(unsafe_code)]`), rustix-only, CI-testable everywhere.

## Next — the privileged backends (trigger: a header-forging protocol crate)

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
- [ ] **`spoof_udp` / `forge_arp` examples** — port from the reference once the L3/L2 backends
      and the `ip`/`udp`/`arp` protocol crates exist to compose real packets.
- [ ] **Concrete compose tests** with the real IP/UDP `Protocol` impls (the current
      `tests/compose.rs` uses an in-test toy stack).

## Open decisions

- [ ] **Dependency in `protocols`** — wire `protocols`' `[workspace.dependencies]` to depend
      on `rawsock` (git dep, mirroring `bnb`) when the first protocol crate implements
      `Protocol` (UDP is the trigger). Not consumed yet.
- [ ] **rustix version** — track 1.x; re-check the L2 link-address story (above) before the
      `link` backend.
- [ ] **macOS / Windows backends** — additive behind `RawIo`; `OpenError::DriverMissing`
      reserved for the Windows (WinDivert/Npcap) path.
