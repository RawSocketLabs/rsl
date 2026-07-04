# rsl — the RawSocket Labs stack

A single Cargo workspace for the public RSL stack. The crates are **published independently**
(you can depend on just `bitsandbytes`, or `rawsock`, or `dns`), but they're **developed in
lockstep** — inter-crate dependencies are in-workspace `path` deps, so there's one lockfile and
no cross-repo version pinning to keep in sync.

## Crates

- **`bitsandbytes`** (`bitsandbytes/bnb`) + **`bitsandbytes-macros`** — the owned, bit-aware
  binary codec (imported as `bnb`). `no_std`, zero `unsafe`.
- **`rawsock`** — dual-use, layered raw-packet I/O (L2/L3/L4).
- **`rfus`** — RF frequency / sample-rate / scan-target parsing.
- **`protocols/*`** — from-scratch, dual-use protocol codecs on `bnb`: `ethertype`, `ethernet`,
  `arp`, `tcp`, `udp`, `ip`, `icmp`, `dns`.
- **`rsl`** — the owned-library facade: one feature-gated re-export of the crates above.
- **`rsl-deps`** — the blessed external-dependency stack (pins + re-exports third-party crates).
- **`usdr`, `rust-dsdcc`** — FFI/SDR bindings (excluded from the default build; need a C++
  toolchain).

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
