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

## Scope notes

- Compliant-by-default but violatable: see `client::raw` and the
  `profile="raw"` annotations. Never harden parsers against representable input.
- GSS-API and SOCKS4/4a are out of scope (recorded decisions).
