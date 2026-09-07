# session/socks

SOCKS wire codecs, beginning with SOCKS5 (RFC 1928 and RFC 1929). refcheck protocol name:
**`socks`**.

> Canonical agent-guidance file. The workspace root [`AGENTS.md`](../../../AGENTS.md) and
> protocol guide [`../../AGENTS.md`](../../AGENTS.md) also apply.

## Status

The default feature set provides pure SOCKS5 wire codecs: method negotiation, command and
reply registries, IPv4/domain/IPv6 endpoints, requests, replies, and RFC 1929 username/password
messages. Optional `blocking` and `tokio` features add CONNECT clients, embeddable server
handshakes, complete per-connection proxies, and bounded listeners. BIND, UDP ASSOCIATE,
GSS-API, SOCKS4, and SOCKS4A remain absent. [`DESIGN.md`](DESIGN.md) records decisions and limits.

## Architecture

- `src/v5/` owns the RFC 1928 and RFC 1929 wire types. It performs no I/O and selects no runtime.
- Wire constants remain stored and public. Builders default them correctly; verbatim encoding
  preserves deviations and canonical encoding repairs them.
- `Endpoint` owns `ATYP`, address bytes, and port so compliant typed construction cannot create
  the draft's mismatched `address_type`/address pair.
- `src/session.rs` shares strict session validation, phase framing, and explicit policies.
- `src/blocking.rs` uses standard I/O; `src/asynchronous.rs` uses optional workspace Tokio.
  Both leave payload decoding/encoding to bnb and preserve unread tunnel bytes.
- Full proxies require destination authorization on each resolved numeric target before dial.
  Embedded `accept` only authenticates/reads; its caller must authorize and connect before
  acknowledging success. Never log credentials or make no-auth an implicit fallback.

## Testing

- `unit` tests live beside pure registry and helper logic.
- `tests/contract.rs` contains byte-exact RFC-derived message shapes.
- `tests/adversarial.rs` covers truncation, unsupported address types, invalid constants,
  unassigned codes, lengths, and parser no-panic behavior.

- `tests/session_blocking.rs` and `tests/session_async.rs` exercise RFC transcripts, hostile
  peers, and loopback composition. Transcript tests are the fast tier; loopback, fuzz, and
  mutation checks are the full tier. Loopback is confined to ephemeral local listeners;
  there are no third-party service dependencies or sleeps in the tests.
- `fuzz/` fuzzes both client/server input through the shared phase framer and bnb decoder.

Run default, `--features blocking`, `--features tokio`, and `--all-features` SOCKS tests.
Also run strict all-target/all-feature SOCKS Clippy, workspace checks from the root guide,
and `cargo +nightly fuzz run --fuzz-dir protocols/session/socks/fuzz session
--target x86_64-unknown-linux-gnu -- -runs=2000000 -max_len=2048`.

## Scope notes

`#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` are on. The default stays runtime-free.
Do not copy draft transport or
test infrastructure wholesale; recover behavior and evidence at the smallest owning layer.
