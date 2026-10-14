# rsl — the RawSocket Labs stack

A single Cargo workspace for the public RSL stack. The crates are **published independently**
(you can depend on just `bitsandbytes`, or `rawsock`, or `dns`), but they're **developed in
lockstep** — inter-crate dependencies are in-workspace `path` deps, so there's one lockfile and
no cross-repo version pinning to keep in sync.

## Crates

- **`bitsandbytes`** (`codecs/bitsandbytes/bnb`) + **`bitsandbytes-macros`** — the owned, bit-aware
  binary codec (imported as `bnb`). `no_std`, zero `unsafe`.
- **`rawsock`** (`transports/rawsock`) — dual-use, layered raw-packet I/O (L2/L3/L4).
- **`usb`** (`transports/usb`) — owned USB host access (Linux usbfs backend; unpublished).
- **`crypto`** (`crypto/core/`) — accuracy-first cryptographic primitives and protection
  contracts, including a readable AES-128-GCM path designed to make every byte transformation
  inspectable and independently testable.
- **`crypto-legacy`** (`crypto/legacy/`) — separately opted-in historical and broken
  primitives for controlled interoperability, capture decoding, fixtures, and teaching; it is
  never included by the facade's bundles.
- **`asn1`**, **`x509`**, **`pki`** (`pki/*`) — strict DER transport, borrowed
  certificate syntax with exact signed-byte preservation, and fail-closed path validation.
- **`compression`** (`codecs/compression/`) — explicit stateful compression/decompression contracts.
- **`error-correction`** (`codecs/error-correction/`) — error-correction encoding/decoding contracts
  with correction reports rather than a misleading lossless-transform abstraction.
- **`rfus`** (`rf/rfus`) — RF frequency / sample-rate / scan-target parsing.
- **`devices/*`** — pure-Rust device drivers with their simulators, parity oracles and shared
  chip crates (unpublished; see `devices/AGENTS.md`).
- **`protocols/*`** — from-scratch, dual-use protocol implementations on `bnb`: the OSI-layer
  crates `ethertype`, `ethernet`, `arp`, `tcp`, `udp`, `ip`, `icmp`, and `dns`, plus
  **`netlink`** (`protocols/system/netlink`) for strict Linux route/generic-netlink
  transport and typed link, address, route, rule, and WireGuard operations.
- **`rsl`** — the owned-library facade: one feature-gated re-export of the crates above.
- **`deps`** — the blessed external-dependency stack (pins + re-exports third-party crates).
- **`usdr-ffi`, `rust-dsdcc`, `bladerf`, `bladerf-sys`** (`bindings/*`) — FFI/SDR bindings
  (excluded from the default build; need native toolchains/libraries).

## Engineering tooling

- **`tools/rust-skills`** — the canonical, independently versioned RSL Rust engineering
  skills, adoption templates, generated agent adapters, and eval fixtures. It has its own
  Cargo workspace and does not activate skills merely by being present in this repository.

## Consuming

Depend on an individual crate, or on the `rsl` facade for the owned libraries and `deps`
for blessed externals. See each crate's README and `rsl/README.md`.

## Developing

```sh
cargo build --workspace       # everything except the FFI members
cargo test --workspace
```

FFI members build on their own (`cargo build --manifest-path bindings/usdr/Cargo.toml`) given a native
toolchain. The [bladeRF workspace](bindings/bladerf/README.md) additionally requires `libbladeRF`
development headers/library and `libclang`. See `AGENTS.md` for the workspace-wide standards.

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
Imported bladeRF bindings retain their upstream [MIT license](bindings/bladerf/LICENSE).
