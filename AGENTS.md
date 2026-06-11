# application/tftp

TFTP file-transfer library (client **and** server) — RFC 1350 with the option
extensions of RFC 2347/2348/2349. protoref protocol name: **`tftp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace
> root `AGENTS.md` (build, protoref workflow, dual-use philosophy) also applies.

## Start here, not by reading the crate

- Open requirements / status: `RUSTC_WRAPPER= cargo run -q -p protoref --
  coverage --protocol tftp`
- What the spec says: `… protoref -- explain <id> --protocol tftp` /
  `… requirements --protocol tftp --doc rfc1350 --section <n>`
- Where a requirement lives: the `//~` annotations — `… protoref -- scan
  --protocol tftp` lists them with `file:symbol`. Jump there.

Keep `protoref check --protocol tftp` clean after any change.

## Three-layer, dual-stance shape

The crate mirrors the SOCKS crate's layering — read `session/socks` first if you
want the worked reference.

- **Low** `src/wire/` — every packet as a typed value, each with
  `decode(&[u8])`/`encode()`. Fixed packets (`Ack`, `Data`, `ErrorPacket`) use
  binrw with an opcode magic; the variable, NUL-terminated ones (`Request`,
  `OptionAck`) are hand-decoded via `wire/codec.rs` (`ByteReader`). TFTP runs
  over UDP, so a packet is one whole datagram — there is no streaming-framing
  problem, decoders parse a complete buffer. `Packet` is the opcode-tagged union.
- **Mid** `src/client/raw.rs` — `RawClient`: validation-free send/recv plus
  `malformed` generators (dual-use escape hatch).
- **High** `src/client/client.rs` — `Client::get`/`put`; `src/server/` —
  `Server` + the `Handler` backend (`MemoryStore` ships for tests).

Shared state-machine code lives in `src/transfer.rs` (`send_file`, `recv_file`,
`recv_packet`, `send_error`, constants) and is used by **both** the client and
the server — the client downloads with `recv_file` and uploads with `send_file`;
the server is the mirror. Touching the retransmission/TID logic changes both
sides at once, so re-run the e2e matrix.

## Feature `options`

`options` (default on) gates RFC 2347–2349 negotiation: the `wire::OptionAck`
packet, `Request.options`, `TftpOption`, the client's `block_size`/`timeout_option`/
`request_tsize` setters, and the server's OACK path. The core RFC 1350 transfer
works with `default-features = false`. Always test both:

```bash
RUSTC_WRAPPER= cargo test -p tftp
RUSTC_WRAPPER= cargo test -p tftp --no-default-features
```

Version-specific doc examples are not needed (no version split), but
options-only behavior is gated in tests with `#[cfg(feature = "options")]`.

## Entry points

- `src/wire/` — `Opcode`, `TransferMode`, `ErrorCode`, `Request`/`RequestKind`,
  `Data`, `Ack`, `ErrorPacket`, `Packet`; with `options`, `OptionAck` /
  `TftpOption`.
- `src/netascii.rs` — CR-LF ↔ LF translation for `netascii` mode.
- `src/client/` — `Client` (high) and `client::raw::RawClient` (mid) +
  `raw::malformed`.
- `src/server/` — `Server`, the `Handler` trait + `Reject`, and `MemoryStore`.
- `src/transfer.rs` — shared lock-step primitives.
- `src/error.rs` — `TftpError`.

## Testing & performance

- `smoke.rs` — one byte round-trips up and down.
- `api.rs` — public surface is reachable and keeps shape.
- `contract.rs` — golden RFC 1350/2347 wire vectors, both directions.
- `integration.rs` — handler rejections → peer errors.
- `e2e.rs` — full transfers across every block-boundary size, netascii, and
  (with `options`) blksize/tsize negotiation.
- `regression.rs` — fresh-TID per transfer, stray-TID ERROR(5), empty-filename
  validation, exact-block-multiple terminator. `//~ verifies` lives here.
- `common/mod.rs` — spawns a memory-backed server; `client()` / `spawn_server()`.

Run everything: `RUSTC_WRAPPER= cargo test -p tftp`.

Benchmarks (`benches/tftp_bench.rs`, criterion + pprof; manual `main`):

- Measure: `cargo bench -p tftp --bench tftp_bench` (groups: `parse`,
  `transfer`).
- Flamegraphs: `… -- --profile-time 5`.
- `--features bench-slow` adds a 1 MiB download arm.

Examples: `cargo run -p tftp --example serve_and_transfer` (full client+server
demo); `… --example decode_packet` (wire layer only).

## Scope notes

- Compliant-by-default but violatable: see `client::raw` and the `profile="raw"`
  annotations. Never harden the codec against representable input — unknown
  opcodes/codes/modes are `Custom`, and no length limit is imposed on a block.
- `mail` mode is modeled (a `TransferMode` variant) but not driven — it is
  obsolete. The host text format for `netascii` is assumed to be Unix LF.
- `module_inception` warnings (`client/client.rs`, `server/server.rs`) are
  accepted house style, matching `session/socks`.
