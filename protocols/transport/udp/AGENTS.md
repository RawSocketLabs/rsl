# transport/udp

UDP (RFC 768) datagram-**header** codec on `bnb`. refcheck protocol name: **`udp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec

Decode/encode of the 8-byte header (four 16-bit fields). No socket or I/O — a wire codec.

## Architecture

- **`UdpHeader`** — `#[bin(big)]`: `src_port`, `dst_port`, `length`, `checksum`.
  `for_payload(src, dst, payload_len)` computes `length = 8 + payload_len` (saturating);
  `payload_len()` is `length - 8` (`saturating_sub`, so a malformed `length < 8` yields 0,
  never underflows); `HEADER_LEN = 8`.

## Dual-use

`length` and `checksum` are stored **verbatim** — decode never recomputes, verifies, or
rejects them, so a forged length or checksum survives a round-trip.

## Scope / follow-ups

- **Checksum is stored, not computed.** A `udp_checksum` (over the IPv4/IPv6 pseudo-header)
  lands with `rawsock`'s composition model (`internet_checksum`).
- **The `rawsock` injection-`Protocol` impl** — UDP is the socket layer's first on-the-wire
  consumer (the ROADMAP's `rawsock` extraction trigger). It waits on `rawsock` being
  published; once it is, add the `Protocol` impl + the `rawsock` git dep here.

## Testing

- `unit` (inline): `for_payload` length math, `payload_len` saturation.
- `tests/integration.rs`: a golden DNS-query header (field-correct, byte-identical round-trip);
  a forged `length` preserved verbatim.
- `tests/adversarial.rs`: truncated header, `length < 8` (no underflow), arbitrary-bytes-never-panic.
- Example: `decode_datagram` (decode + build).

Run: `cargo test -p udp`.

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
