# rawsock — agent guide

Dual-use, layered raw-packet I/O. The **sink** half of the RawSocketLabs workspace: the
`bnb`-based protocol crates encode bytes; `rawsock` transmits them verbatim at a chosen
layer. Single-crate repo (no proc-macros); `CLAUDE.md` is a symlink to this file.

> Sibling repos: [`bitsandbytes`](https://github.com/RawSocketLabs/bitsandbytes) (the codec),
> [`protocols`](https://github.com/RawSocketLabs/protocols) (the protocol crates that consume
> both). This crate is bnb-**independent** — pure I/O + composition.

## Status — full L2→L4 injection

Shipped: `RawIo`/`Layer`/`OpenError`, the `compose` model (`Protocol`/`ProtocolExt`/
`Context`/`Pseudo`/`internet_checksum`), the `Loopback` backend, `capabilities()` probing, and
all three socket rungs — `transport` (L4 UDP), `network` (L3 `NetworkSocket`, raw IPv4 via
`IPPROTO_RAW`), and `link` (L2 `LinkSocket`, raw Ethernet via `AF_PACKET`, bound by interface
name). The L3/L2 backends are privileged (`CAP_NET_RAW`) and are the rungs the `protocols`
`ip`/`ethernet` layers target to put forged datagrams and frames on the wire.

**One confined `unsafe`:** rustix 1.1.4 exposes no safe link-layer socket address, so
`linux/link.rs` builds a `sockaddr_ll` via `SocketAddrAny::read` inside a single
`#[allow(unsafe_code)]` fn. The crate is otherwise `#![deny(unsafe_code)]` (relaxed from
`forbid` for exactly this, per the ROADMAP) — **no `libc`**, rustix-only.

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
  `pnet`/`pcap` — **rustix-only, no `libc`**.
- The crate is `#![deny(unsafe_code)]` with **exactly one** `#[allow(unsafe_code)]`, the
  `link_addr` fn in `linux/link.rs` (the `link`/L2 backend). rustix 1.1.4 (the latest) has no
  safe link-layer socket address, so binding an `AF_PACKET` socket needs one call to build a
  `sockaddr_ll` via `SocketAddrAny::read`. `name_to_index` covered the interface lookup, so
  `libc` was avoided entirely. (This is the pre-planned relaxation from `forbid` → `deny`;
  keep the `unsafe` confined to that one fn — never widen it.)

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
