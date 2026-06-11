# session/socks

SOCKS5 proxy library — RFC 1928 (CONNECT, BIND, UDP ASSOCIATE) and RFC 1929
(username/password auth). protoref protocol name: **`socks`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace
> root `AGENTS.md` (build, protoref workflow, dual-use philosophy) also applies.

## Start here, not by reading the crate

- Open requirements / status: `RUSTC_WRAPPER= cargo run -q -p protoref --
  coverage --protocol socks`
- What the spec says: `… protoref -- explain <id> --protocol socks` /
  `… requirements --protocol socks --section <n> --level must`
- Where a requirement lives: it's in the `//~` annotations — `… protoref --
  scan --protocol socks` lists them with `file:symbol`. Jump there.

The corpus is fully triaged (0 untriaged). Keep `protoref check --protocol
socks` clean after any change.

## Entry points

- `src/client/` — `Client` (compliant) and `client::raw::RawClient`
  (deliberately malformed frames for testing). `TargetAddr`, BIND, UDP tunnel.
- `src/server/` — `Server`, per-connection handling, TCP relay, UDP relay.
- `src/auth.rs` — `Authenticator`/`AuthHandler` traits + NoAuth and
  username/password. GSS-API (RFC 1961) is deferred to a future helper crate.
- `src/v5/` — wire-format messages (identifier, request/reply, method,
  address, UDP header, user/pass).
- `src/error.rs` — `SocksError`.

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
- `common/mod.rs` — shared spawn/echo/proxy helpers.

Run everything: `RUSTC_WRAPPER= cargo test -p socks`.

Benchmarks (`benches/socks_bench.rs`, criterion + pprof):

- Measure: `cargo bench -p socks --bench socks_bench` (groups: `parse`,
  `handshake`, `relay`; reports under `target/criterion/`).
- Flamegraph SVGs: `cargo bench -p socks --bench socks_bench -- --profile-time 5`.
- Quantify the TCP_NODELAY win: `cargo bench --features bench-nagle` adds the
  deliberately-slow Nagle baseline arm (`relay/nagle/*`) alongside
  `relay/nodelay/*`; off by default so a normal run stays fast.

Coverage (cargo-tarpaulin, config in `tarpaulin.toml`):
`cargo tarpaulin --config tarpaulin.toml` → HTML + LCOV in `target/coverage/`.

## Scope notes

- Compliant-by-default but violatable: see `client::raw` and the
  `profile="raw"` annotations. Never harden parsers against representable input.
- GSS-API and SOCKS4/4a are out of scope (recorded decisions).
