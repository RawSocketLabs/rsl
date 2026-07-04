# rawsock — agent guide

Dual-use, layered raw-packet I/O. The **sink** half of the RawSocketLabs workspace: the
`bnb`-based protocol crates encode bytes; `rawsock` transmits them verbatim at a chosen
layer. Single-crate repo (no proc-macros); `CLAUDE.md` is a symlink to this file.

> Sibling repos: [`bitsandbytes`](https://github.com/RawSocketLabs/bitsandbytes) (the codec),
> [`protocols`](https://github.com/RawSocketLabs/protocols) (the protocol crates that consume
> both). This crate is bnb-**independent** — pure I/O + composition.

## Status — unprivileged core + L3 injection

Shipped: `RawIo`/`Layer`/`OpenError`, the `compose` model (`Protocol`/`ProtocolExt`/
`Context`/`Pseudo`/`internet_checksum`), the `Loopback` backend, `capabilities()` probing,
the `transport` (L4 UDP via rustix) backend, and — the `network` feature — the **L3
`NetworkSocket`** (raw IPv4 via `SOCK_RAW`/`IPPROTO_RAW`, `IP_HDRINCL`; privileged, needs
`CAP_NET_RAW`). It's the rung the `protocols` `ip` layer targets to put forged IP datagrams on
the wire. Deferred: the `link` (L2, `AF_PACKET`) backend — see `ROADMAP.md`.

## Architecture

- **`RawIo`** (lib.rs) — the sink trait: `send_raw` (verbatim), `recv`, `layer`. Backends
  implement it.
- **`compose`** — the layered composition model. `Protocol::{encode_with, encode_raw_with,
  protocol_id, layer}`; stack with a container's `.payload()`; `Vec<u8>` is a leaf.
  `encode()` is compliant (computes lengths/checksums via `Context`/`Pseudo`, gated by the
  `compute` feature); `encode_raw()` is verbatim. `internet_checksum` (RFC 1071).
- **`loopback`** — in-memory `RawIo` (records + replays); carries most unit coverage.
- **`capability`** — probes which layers the host+process can open *right now* (never
  assumed) by attempting a socket per layer.
- **`linux/transport`** — the one shipped socket backend: unprivileged UDP over rustix.

## Dual-use doctrine (load-bearing)

`send_raw` never validates. The `compose` model is compliant *by default* but every derived
field is overridable (`encode_raw`, a per-field override, or `Vec<u8>`-as-payload) so you can
forge a lying length/checksum/demux. Lower layers are opt-in features so the dangerous power
is a deliberate, compile-checked opt-in. Never add validation to the sink.

## Upstream crates & `unsafe`

- **rustix** is the syscall crate (Linux, `net` feature, optional). Not `libc`/`socket2`/
  `pnet`/`pcap`. The shipped core is `#![forbid(unsafe_code)]`.
- The **future `link` backend** is the only planned FFI: `libc` for the `AF_PACKET`
  `sockaddr_ll` bind + `if_nametoindex`, isolated to `linux/link.rs`. **Before adding it,
  check rustix's `netdevice` (`name_to_index`) and any `AF_PACKET`/link-address support** —
  newer rustix (1.x) may cover it and let us stay libc-free. If `libc` is unavoidable,
  relax the crate `forbid(unsafe_code)` to `deny` and confine `unsafe` to that module.

## Testing

- `tests/core.rs` — `Loopback` verbatim replay + `capabilities()` self-consistency.
- `tests/compose.rs` — the composition model via a small **in-test toy stack** (the real
  IP/UDP `Protocol` impls live in the `protocols` workspace, not here): `Context` threading,
  cross-layer checksum, demux auto-set + override, `encode` vs `encode_raw`, `internet_checksum`.
- `tests/transport.rs` — a real unprivileged UDP round-trip on loopback (runs in CI).
- The privileged L3/L2 backends will add namespace-gated tests (`unshare --user
  --map-root-user --net`, `lo` up) — `RAWSOCK_PRIV_TESTS` guards them; unset ⇒ they no-op.

`cargo test` · `cargo clippy --all-targets` · `cargo fmt --all --check` · MSRV 1.85.

## Conventions

Conventional Commits, **lowercase subject** (lead with a verb), **no `Co-Authored-By:`
trailer**. `#![deny(missing_docs)]`. Workspace lints: `clippy::all` denied, `pedantic` warn.
