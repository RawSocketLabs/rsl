# transport/tcp

TCP (RFC 9293) segment-**header** codec on `bnb`. refcheck protocol name: **`tcp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec + rawsock injection

Decode/encode of the 20-byte fixed header plus raw options, plus (the `inject` feature) a
`rawsock::Protocol` layer that composes and injects real, checksummed segments. No connection
state machine, retransmission, or I/O.

## Architecture

- **`Control`** — the data-offset + reserved + control-bits word (byte 12–13), a flat
  `#[bitfield(u16, bits = msb)]`: `data_offset: u4`, `reserved: u4`, then the eight control
  bools MSB-first (`cwr ece urg ack psh rst syn fin`). The RFC bit diagram, packed into one
  `u16` — a `bnb` bitfield showcase.
- **`TcpHeader`** — `#[bin(big)]`: ports, `seq`, `ack`, `control`, `window`, `checksum`,
  `urgent`, and `options: Vec<u8>` sized by `data_offset` (`(data_offset - 5) * 4`, with
  `saturating_sub` so a malformed `data_offset < 5` reads zero option bytes, never panics).
  `TcpHeader::segment(...)` computes `data_offset` from the options length; `header_len()`,
  `is_syn/is_ack/is_fin/is_rst` convenience accessors.

## Dual-use

`checksum`, `reserved`, and `data_offset` are stored **verbatim** — decode never recomputes,
verifies, or rejects them, so a forged checksum or a lying data-offset survives a round-trip.
Options are raw bytes, preserved exactly. The parser enforces no policy.

## Options — raw + a structured view

`TcpHeader.options` stays **raw bytes** (dual-use: any/malformed options preserved exactly).
`options.rs` adds a lens: `TcpOption` (Eol/Nop/Mss/WindowScale/SackPermitted/Sack/Timestamps/
`Unknown{kind,value}`), `options_parsed()` to read them, and `options::encode` to build them.
Parsing is bounded — a truncated/wrong-length option becomes `Unknown` and stops the scan,
never panics.

## Injection — the `inject` feature (`src/inject.rs`)

`Tcp<P>` wraps a `TcpHeader` + payload and implements `rawsock::Protocol` (`protocol_id` → 6,
`layer` → `Transport`). Dual-use: `encode` (compliant) computes the checksum from the enclosing
IPv4 pseudo-header (`Context`); `encode_raw` (verbatim) emits `.header.checksum` as set (forge
by writing it). `tcp_checksum(pseudo, tcp)` is RFC 9293 over the pseudo-header + segment on
rawsock's `internet_checksum` — no UDP-style `0 → 0xFFFF` sentinel. rawsock is a git dep behind
the feature (`default-features = false` → the `compose` trait + `Loopback`, no `rustix`). The IP
layer (`network/ip`) supplies the pseudo-header in a full `Ip(Tcp(...))` stack.

## Scope / follow-ups

- **DNS-over-TCP** (the `dns` resolver's TCP fallback) is a downstream consumer once a stream
  transport exists.

## Testing

- `unit` (inline): `Control` bit packing (offset + MSB-first flags), `segment` data-offset math.
- `tests/integration.rs`: golden headers (a SYN; a header with an MSS option) — field-correct
  and byte-identical round-trips.
- `tests/adversarial.rs`: `data_offset < 5` (no underflow panic), truncated header, options
  past the buffer, arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`): `protocol_id`/`layer`; **the checksum verifies to 0**
  (RFC 1071); raw preserves a forged checksum; segment data rides after the header; the composed
  segment goes out a `Loopback` sink.
- Examples: `decode_segment` (print ports/flags), `build_syn` (construct + round-trip),
  `inject_segment` (`--features inject`: compose a SYN, checksum, send through a rawsock sink).

Run: `cargo test -p tcp` (add `--features inject` for the rawsock layer).

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
