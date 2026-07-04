# `rsl-deps` — the RSL blessed external-dependency stack

One feature-gated crate that pins and re-exports the third-party crates RawSocket Labs
libraries and applications build on. Depend on `rsl-deps` instead of pinning `thiserror`,
`tokio`, `serde`, … by hand in every crate — versions are unified here, in one place.

```toml
[dependencies]
rsl-deps = { git = "https://github.com/RawSocketLabs/rsl-deps", features = ["std-ext", "service"] }
```

```rust
use rsl_deps::prelude::*;   // anyhow::Result, tracing macros, serde derives
use rsl_deps::tokio;        // blessed async runtime, canonical name
use rsl_deps::serde_json;
```

## Deps vs. libraries

RSL's stack is split by concern:

- **`rsl-deps`** (this crate) — the third-party crates we *rent*. A single source of truth for
  their versions, and a standing shortlist of replacement candidates.
- **[`rsl`](https://github.com/RawSocketLabs/rsl)** — the libraries we *own* (`codec`, `proto`,
  `rawsock`, `rf`, `usdr`). Consumers wanting RSL's own products depend on that.
- **`rsl-private`** — the private owned libraries, layered on `rsl`.

The two are orthogonal: a crate that needs both blessed externals and RSL libraries depends on
both `rsl-deps` and `rsl`. When an owned crate supersedes a rented one, the pin moves out of
`rsl-deps` into `rsl`.

## Features

Nothing compiles until you enable a feature. Each capability re-exports one or more crates under
their canonical names (`rsl_deps::<crate>`), plus a `prelude` of everyday items.

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

Bundles: `std-ext` (`error`+`log`+`serde`), `service` (`async`+`nats`+`parallel`+`std-ext`+`bytes`),
`web` (`web-server`+`openapi`), and `full` (everything).

## Notes

- **`publish = false` for now** — intended to move to a registry (crates.io / private) so that
  published RSL libraries can depend on it. It has no git deps of its own (only registry version
  pins), so it's already publishable once that decision is made.
- **MSRV floors on features** — core is Rust 1.85; `tui` (`ratatui`) and `time` (`chrono`) pull
  crates needing **1.88**. Enabling them raises the effective MSRV.

## Verify

```sh
cargo build --features full
```
