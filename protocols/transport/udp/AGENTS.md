# transport/udp

UDP (RFC 768) datagram-**header** codec on `bnb`. refcheck protocol name: **`udp`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — header codec + rawsock injection

Decode/encode of the 8-byte header (four 16-bit fields), plus (the `inject` feature) a
`rawsock::Protocol` layer that composes and injects real, checksummed packets.

## Architecture

- **`UdpHeader`** — `#[bin(big)]`: `src_port`, `dst_port`, `length`, `checksum`.
  `for_payload(src, dst, payload_len)` computes `length = 8 + payload_len` (saturating);
  `payload_len()` is `length - 8` (`saturating_sub`, so a malformed `length < 8` yields 0,
  never underflows); `HEADER_LEN = 8`.

## Dual-use

`length` and `checksum` are stored **verbatim** — decode never recomputes, verifies, or
rejects them, so a forged length or checksum survives a round-trip.

## Injection — the `inject` feature (`src/inject.rs`)

`Udp<P>` wraps a `UdpHeader` + payload and implements `rawsock::Protocol` (`protocol_id` →
17, `layer` → `Transport`) — the socket layer's **first on-the-wire consumer** (the ROADMAP's
`rawsock` extraction trigger). Dual-use, matching rawsock's two encode paths:
- `encode` (compliant): recomputes `length` from the payload and the checksum from the
  enclosing IPv4 pseudo-header (`Context`); with no pseudo-header the checksum stays 0 (a
  legal IPv4 "no checksum").
- `encode_raw` (verbatim): emits `.header`'s `length`/`checksum` exactly as set — forge them
  by writing the fields.

`udp_checksum(pseudo, udp)` is RFC 768 over the pseudo-header + datagram, on rawsock's
`internet_checksum`; a computed `0x0000` becomes `0xFFFF`. rawsock is a **git dep behind the
feature** with `default-features = false` (the `compose` trait + `Loopback` only — no `rustix`
socket code). Follow-ups: a TCP checksum helper; actual privileged L3 send (needs rawsock's
`network` backend + an IP layer to wrap `Udp` in).

## Testing

- `unit` (inline): `for_payload` length math, `payload_len` saturation.
- `tests/integration.rs`: a golden DNS-query header (field-correct, byte-identical round-trip);
  a forged `length` preserved verbatim.
- `tests/adversarial.rs`: truncated header, `length < 8` (no underflow), arbitrary-bytes-never-panic.
- `tests/inject.rs` (`--features inject`): `protocol_id`/`layer`; compliant encode computes
  length + checksum; **the checksum verifies to 0** (RFC 1071); no-pseudo → checksum 0; raw
  preserves forged fields; the composed packet goes out a `Loopback` `RawIo` sink.
- Examples: `decode_datagram` (decode + build); `inject_packet` (`--features inject`: compose,
  checksum, send through a rawsock sink).

Run: `cargo test -p udp` (add `--features inject` for the rawsock layer).

## Conventions

Conventional Commits (lowercase, verb-led subject), no `Co-Authored-By:`. `#![forbid(unsafe_code)]`,
`#![deny(missing_docs)]`. Workspace lints (`clippy::all` denied).
