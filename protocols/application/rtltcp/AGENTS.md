# application/rtltcp

The `rtl_tcp` protocol (an RTL-SDR served over TCP) on `bnb`. No RFC; protocol reference:
<https://k3xec.com/rtl-tcp/>. refcheck protocol name: none.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — codec + blocking client

The server's 12-byte **greeting** and the client's 5-byte **commands**, both big-endian
`#[bin]` types, plus an optional **`client` feature**: a blocking [`Client`] on std's
`TcpStream` (`src/client.rs`). The client reads the greeting, sends commands, and reads the
raw interleaved CU8 sample stream into caller buffers. It applies no device policy (gain
choice, retune settle windows, sample normalization); consumers such as libsdr own that.
Deferred: a server half, async clients.

## Architecture

- `tuner_info.rs` — `TunerInfo { tuner_type, gain_count }`, the greeting's payload, with `RTL0`
  as the `#[bin]` magic.
- `command.rs` — `Command { opcode, param }` and `Opcode`, a `BitEnum` with an `Other`
  catch-all. Typed constructors cover the opcodes libsdr drives; `Command::new` sends any.
- `client.rs` — `Client` and `ClientError`. The greeting is decoded straight off the socket
  through a `StreamBitReader`, which reads exactly the message's bytes. bnb's `MessageStream`
  is not used: the server sends one message (the greeting) and then only raw samples, and
  never replies to a command, so there is no message stream to frame — the wrapper would only
  add a buffer that the sample reads, `drain_buffered` and `discard` would all have to go
  through. Commands go out with `EncodeExt::encode`.

## The dual-use rule here

The parser accepts any opcode (preserved as `Opcode::Other`). The greeting's `RTL0` is a
codec constant rather than a field, so decoding any other magic fails with `BadMagic`, which
the client reports as `ClientError::Handshake`.

## Testing

- **`unit` / `component`** — inline in `tuner_info.rs` and `command.rs`.
- **`contract`** — `tests/contract.rs`: golden bytes for the greeting and every typed command.
- **`adversarial`** — `tests/adversarial.rs`: truncation, foreign magic, unknown opcodes.
- **`client`** — `tests/client.rs` (`--features client`): real loopback TCP against a scripted
  server thread.

Run everything: `cargo test -p rtltcp --features client`.
