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

- [ ] **`network` (L3)** — raw IP via `SOCK_RAW`/`IPPROTO_RAW` (`IP_HDRINCL` free). Lands with
      the first crate that hand-builds IP headers (IP/ICMP). Namespace-gated tests.
- [ ] **`link` (L2)** — raw Ethernet via `AF_PACKET`. Lands with ARP/Ethernet. **Before
      writing it, re-evaluate rustix 1.x**: its `netdevice` module (`name_to_index`) and any
      link-address support may cover the `sockaddr_ll` bind + `if_nametoindex` that the
      reference used `libc` for — potentially keeping the whole crate libc-free. If `libc` is
      unavoidable, relax the crate `forbid(unsafe_code)` → `deny` and confine `unsafe` to
      `linux/link.rs`.
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
