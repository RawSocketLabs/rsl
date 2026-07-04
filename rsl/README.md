# `rsl` — the RawSocket Labs stack

A single, feature-gated facade over the crates RSL application layers build on. Depend on
`rsl`, enable the features you need, and get a curated, version-unified slice of the stack —
instead of pinning a dozen internal and external crates by hand in every application repo.

```toml
[dependencies]
rsl = { git = "https://github.com/RawSocketLabs/rsl", features = ["net", "std-ext"] }
```

```rust
use rsl::prelude::*;          // anyhow::Result, tracing macros, serde derives, codec…
use rsl::proto::dns;          // owned protocol crate
use rsl::codec;               // bnb (bitsandbytes)
use rsl::ext::serde_json;     // blessed external, canonical name
```

## Why route the stack through one crate

- **One place to change versions.** Every blessed pin lives in this crate's `Cargo.toml`.
  Bump it once and every application crate that consumes `rsl` moves together. No more
  per-repo version drift.
- **Owned vs. external is explicit.** Crates RSL owns sit under semantic paths (`rsl::codec`,
  `rsl::proto`, …). Blessed third-party crates sit under `rsl::ext::*`. That split
  *is* the answer to "what do we depend on, and what might we replace?"
- **Pick only what you compile.** Nothing builds until you enable a feature; `rsl` with
  default features is empty.

## Namespace map

### Owned RSL crates

Sourced by `git` from their public `RawSocketLabs` repos.

| Path | Backed by | Feature |
|------|-----------|---------|
| `rsl::codec` | `bitsandbytes` (`bnb`) — bit-aware binary codec | `codec` |
| `rsl::proto::{ethertype,tcp,udp,dns}` | the `protocols` workspace | `proto` (or `proto-<name>`) |
| `rsl::rawsock` | `rawsock` — L2/L3/L4 raw-packet I/O | `rawsock` |
| `rsl::rf` | `rfus` — RF/sample-rate/scan-target parsing | `rf` |
| `rsl::usdr` | `usdr` — USDR SDR bindings (FFI, needs C++ toolchain) | `usdr` |

**Not in this public facade** (by design): the private `libsdr` / `rust-dsdcc`. Bless those
in a private overlay crate that depends on `rsl` — see "Public vs. private" below.

### Blessed external crates (`rsl::ext::*`) — replacement candidates

| Feature | Re-exports |
|---------|-----------|
| `error` | `thiserror`, `anyhow` |
| `log` | `tracing`, `tracing_subscriber`, `tracing_appender` |
| `serde` | `serde`, `serde_json` |
| `bytes` | `bytes` |
| `cli` | `clap` |
| `rand` | `rand` |
| `hash` | `sha2`, `crc32fast` |
| `netutil` | `macaddr`, `socket2` |
| `parse` | `winnow` |
| `num` | `num_complex` |
| `audio` | `hound` |
| `fft` | `rustfft` |
| `async` | `tokio`, `futures_util` |
| `nats` | `async_nats` |
| `parallel` | `rayon` |
| `web-server` | `axum`, `tower`, `tower_http` |
| `openapi` | `utoipa`, `utoipa_swagger_ui` |
| `http` | `reqwest` |
| `tui` | `ratatui`, `color_eyre` |
| `time` | `chrono` |
| `protobuf` | `prost` |

### Convenience bundles

| Feature | Pulls |
|---------|-------|
| `net` | `codec`, `proto`, `rawsock`, `bytes`, `error`, `log`, `netutil` |
| `radio` | `rf`, `num`, `audio`, `fft` (RF parsing + SDR/DSP externals; add `usdr` explicitly — FFI) |
| `service` | `async`, `nats`, `parallel`, `std-ext`, `bytes` (async daemons / API services) |
| `web` | `web-server`, `openapi` |
| `std-ext` | `error`, `log`, `serde` |
| `full` | `net` + `radio` + `service` + `std-ext` |

## The graduation path (external → owned)

When RSL writes an owned crate to supersede a blessed external, it **graduates out of
`ext`**: the re-export moves from `rsl::ext::foo` to a semantic path (e.g. `rsl::error`), the
external pin is deleted from `Cargo.toml`, and the owned crate is added as a git dep.
Consumers see the move in one place. Keeping externals quarantined under `ext` is what keeps
that replacement decision legible — the `ext` namespace is the standing shortlist of "things
we currently rent instead of own."

## Public vs. private

This is the **public** facade: blessed externals + owned crates that have **public**
`RawSocketLabs` repos. The private owned crates (`libsdr`, `rust-dsdcc`) are intentionally
excluded so nothing private is named or linked here. The intended
pattern for private crates is a small **private overlay crate** that depends on `rsl` and adds
them under their own semantic paths — public/private boundary = repo boundary.

## Sourcing model & caveats

- **Owned crates are `git` deps** to their public `RawSocketLabs` repos, pinned to `branch =
  "main"` for now. Move to a `rev`/`tag` once churn settles. Sourcing is an implementation
  detail — the public `rsl::…` paths consumers import do not change when a pin moves.
- **One codec, no patch.** `bnb` is pulled from the same git source the protocol crates use,
  so a consumer enabling both `codec` and `proto` compiles exactly one `bitsandbytes` whose
  types unify.
- **`publish = false`.** Git dependencies can't appear in a crates.io release, so `rsl` is
  consumed directly as a git dependency, not published.
- **FFI feature `usdr` needs a C++ toolchain** and is off by default.
- **MSRV floors on features.** The core stack is Rust 1.85. Some optional features pull crates
  with a higher floor — notably `tui` (`ratatui`) and `time` (`chrono`) require **1.88**.
  Enabling them raises the effective MSRV; the 1.85 floor applies to the feature-free core.

## Verify

```sh
# pure-Rust slice (no FFI):
cargo build --features codec,proto,rawsock,std-ext,bytes,netutil,hash,parse,num,audio,fft
```
