# session/socks

SOCKS proxy library covering all three wire versions, each behind a Cargo
feature. refcheck protocol name: **`socks`**.

| Version | Feature | Source | Surface |
|---|---|---|---|
| SOCKS5 | `v5` *(default)* | RFC 1928 / 1929 | `client`/`server`, `auth`, `v5` |
| SOCKS4 | `v4` | 1992 Ying-Da Lee memo | `client::v4`/`server::v4`, `v4` |
| SOCKS4A | `v4a` (⊃ `v4`) | 4A extension memo | adds domain targets/resolution |

Default build is `v5` only (the crate began SOCKS5-only). The version features
are additive and per-version types keep each surface honest (no UDP/auth on v4).
At least one version feature must be enabled; `error::SocksError` and the
`server::{relay,pool}` internals are the only version-agnostic parts.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace
> root `AGENTS.md` (build, refcheck workflow, dual-use philosophy) also applies.

## Feature matrix — always test more than the default

The default `cargo test -p socks` exercises **only v5**. When touching shared
code (`error`, `server/pool`, `server/relay`, module gating), run the matrix:

```bash
RUSTC_WRAPPER= cargo test -p socks                                    # v5 (default)
RUSTC_WRAPPER= cargo test -p socks --no-default-features --features v4
RUSTC_WRAPPER= cargo test -p socks --no-default-features --features v4a
RUSTC_WRAPPER= cargo test -p socks --no-default-features --features v4a,v5
```

Version-specific doc examples are gated inside the snippet with
`# #[cfg(feature = "v5")] {` … `# }` so doctests pass under every config.

## Start here, not by reading the crate

- Open requirements / status: `RUSTC_WRAPPER= cargo run -q -p refcheck --
  coverage --protocol socks`
- What the spec says: `… refcheck -- explain <id> --protocol socks` /
  `… requirements --protocol socks --section <n> --level must`
- Where a requirement lives: it's in the `//~` annotations — `… refcheck --
  scan --protocol socks` lists them with `file:symbol`. Jump there.

The corpus is fully triaged (0 untriaged). Keep `refcheck check --protocol
socks` clean after any change.

## Entry points

SOCKS5 (`v5`):
- `src/client/` — `Client` (compliant) and `client::raw::RawClient`
  (deliberately malformed frames for testing). `TargetAddr`, BIND, UDP tunnel.
- `src/server/` — `Server`, per-connection handling, TCP relay, UDP relay.
- `src/auth.rs` — `Authenticator`/`AuthHandler` traits + NoAuth and
  username/password. GSS-API (RFC 1961) is deferred to a future helper crate.
- `src/v5/` — wire-format messages (identifier, request/reply, method,
  address, UDP header, user/pass).

SOCKS4 / 4A (`v4` / `v4a`):
- `src/v4/` — wire-format messages: `Request` (VN/CD/DSTPORT/DSTIP/USERID and,
  under `v4a`, a trailing domain), `Reply`, `Command`, `ReplyCode`. `Request`
  has a hand-written `BinWrite`/`read_from` (binrw's attribute macro doesn't
  honor `#[cfg]` on the conditional `domain` field).
- `src/client/v4/` — `Client` (`userid`, `connect`, `connect_domain` [4a],
  `bind`) and `client::v4::raw::RawClient`.
- `src/server/v4/` — `Server` with an `with_authorizer` access-control hook,
  CONNECT/BIND, and 4A domain resolution. Reuses `server::{relay,pool}`.

Shared:
- `src/error.rs` — `SocksError` (version-gated variants: `ReplyFailure`/auth are
  `v5`; `V4ReplyFailure` is `v4`).
- `src/server/pool.rs` — `Semaphore` + `accept_with_timeout`, used by both
  servers.

## Testing & performance

Tests are split by purpose so any layer can run in isolation. Unit / component
/ sanity tests live in-source under `#[cfg(test)] mod unit` next to the code;
the rest are dedicated binaries under `tests/`:

- `smoke.rs` — liveness (one byte round-trips through the proxy).
- `api.rs` — public surface is reachable and keeps its shape.
- `contract.rs` — golden RFC 1928/1929 wire vectors, parse direction.
- `integration.rs` — client/server negotiation and error mapping.
- `e2e.rs` — full proxy→target flows (CONNECT, BIND, UDP, auth).
- `regression.rs` — guards for fixed defects (UDP source hijack, BIND /
  handshake timeouts, client-IP pinning). `//~ verifies` annotations live here.
- `observability.rs` — asserts `tracing` events fire under a subscriber (its
  own binary on purpose: tracing caches callsite interest on first hit).
- `v4_contract.rs` — golden SOCKS4 / 4A wire vectors (`v4` only).
- `v4_e2e.rs` — full SOCKS4 / 4A proxy→target flows: CONNECT, BIND, reply-code
  mapping, the authorizer hook, the raw escape hatch, 4A domain resolution.
- `common/mod.rs` — shared helpers; `spawn_tcp_echo` is version-agnostic, the
  rest are split into `v5_helpers` / `v4_helpers` submodules.

The v5 test binaries carry `#![cfg(feature = "v5")]` and `v4_*` carry
`#![cfg(feature = "v4")]`, so the whole `tests/` tree compiles under any single
version feature.

Run everything: `RUSTC_WRAPPER= cargo test -p socks` (v5; see the feature matrix
above for the other versions).

Benchmarks (`benches/socks_bench.rs`, criterion (shared `testutil::bench`)):

- Measure: `cargo bench -p socks --bench socks_bench` (v5 groups: `parse`,
  `handshake`, `relay`; reports under `target/criterion/`). The `v4` feature
  adds a `v4/parse` group; the bench uses a manual `main` so groups are
  selected by feature.
- Quantify the TCP_NODELAY win: `cargo bench --features bench-nagle` adds the
  deliberately-slow Nagle baseline arm (`relay/nagle/*`) alongside
  `relay/nodelay/*`; off by default so a normal run stays fast.

Coverage (cargo-tarpaulin, config in `tarpaulin.toml`):
`cargo tarpaulin --config tarpaulin.toml` → HTML + LCOV in `target/coverage/`.

## Scope notes

- Compliant-by-default but violatable: see `client::raw` / `client::v4::raw`
  and the `profile="raw"` annotations. Never harden parsers against
  representable input.
- GSS-API (RFC 1961) remains out of scope, deferred to a future helper crate.
- SOCKS4 / 4A are **in scope** as of the `v4` / `v4a` features (superseding the
  earlier out-of-scope decision). The 4/4A corpus is the two openssh.com memos,
  added to `manifest.json` as authoritative `url` sources.
