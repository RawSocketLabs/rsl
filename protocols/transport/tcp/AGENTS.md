# transport/tcp

TCP (RFC 9293) segment-**header** codec on `bnb`. refcheck protocol name: **`tcp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec

Decode/encode of the 20-byte fixed header plus raw options. No connection state machine,
retransmission, or I/O — this is a wire codec.

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

## Scope / follow-ups

- **Checksum is stored, not computed.** A `tcp_checksum` (over the IPv4 pseudo-header, like
  UDP) lands with `rawsock`'s composition model (`internet_checksum`).
- **DNS-over-TCP** (the `dns` resolver's TCP fallback) is a downstream consumer once a stream
  transport exists.

## Testing

- `unit` (inline): `Control` bit packing (offset + MSB-first flags), `segment` data-offset math.
- `tests/integration.rs`: golden headers (a SYN; a header with an MSS option) — field-correct
  and byte-identical round-trips.
- `tests/adversarial.rs`: `data_offset < 5` (no underflow panic), truncated header, options
  past the buffer, arbitrary-bytes-never-panic.
- Examples: `decode_segment` (print ports/flags), `build_syn` (construct + round-trip).

Run: `cargo test -p tcp`.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
