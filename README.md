# rsl — the RawSocket Labs stack

A single Cargo workspace for the public RSL stack. The crates are **published independently**
(you can depend on just `bitsandbytes`, or `rawsock`, or `dns`), but they're **developed in
lockstep** — inter-crate dependencies are in-workspace `path` deps, so there's one lockfile and
no cross-repo version pinning to keep in sync.

## Crates

- **`bitsandbytes`** (`bitsandbytes/bnb`) + **`bitsandbytes-macros`** — the owned, bit-aware
  binary codec (imported as `bnb`). `no_std`, zero `unsafe`.
- **`rawsock`** — dual-use, layered raw-packet I/O (L2/L3/L4).
- **`rsl-netlink`** — strict Linux route/generic-netlink transport with typed link,
  address, route, rule, and WireGuard operations; fixed wire framing uses `bitsandbytes`.
- **`rsl-crypto`** (`crypto/`) — accuracy-first cryptographic primitives and protection
  contracts, including a readable AES-128-GCM path designed to make every byte transformation
  inspectable and independently testable.
- **`rsl-crypto-legacy`** (`crypto-legacy/`) — separately opted-in historical and broken
  primitives for controlled interoperability, capture decoding, fixtures, and teaching; it is
  never included by the facade's bundles.
- **`rsl-asn1`**, **`rsl-x509`**, **`rsl-pki`** (`pki/*`) — strict DER transport, borrowed
  certificate syntax with exact signed-byte preservation, and fail-closed path validation.
- **`rsl-compression`** (`compression/`) — explicit stateful compression/decompression contracts.
- **`rsl-error-correction`** (`error-correction/`) — error-correction encoding/decoding contracts
  with correction reports rather than a misleading lossless-transform abstraction.
- **`rfus`** — RF frequency / sample-rate / scan-target parsing.
- **`protocols/*`** — from-scratch, dual-use protocol codecs on `bnb`: `ethertype`, `ethernet`,
  `arp`, `tcp`, `udp`, `ip`, `icmp`, `dns`.
- **`rsl`** — the owned-library facade: one feature-gated re-export of the crates above.
- **`rsl-deps`** — the blessed external-dependency stack (pins + re-exports third-party crates).
- **`usdr`, `rust-dsdcc`** — FFI/SDR bindings (excluded from the default build; need a C++
  toolchain).

## Engineering tooling

- **`tools/rust-skills`** — the canonical, independently versioned RSL Rust engineering
  skills, adoption templates, generated agent adapters, and eval fixtures. It has its own
  Cargo workspace and does not activate skills merely by being present in this repository.

## Consuming

Depend on an individual crate, or on the `rsl` facade for the owned libraries and `rsl-deps`
for blessed externals. See each crate's README and `rsl/README.md`.

## Developing

```sh
cargo build --workspace       # everything except the FFI members
cargo test --workspace
```

FFI members build on their own (`cargo build --manifest-path usdr/Cargo.toml`) given a C++
toolchain. See `AGENTS.md` for the workspace-wide standards.

Licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
