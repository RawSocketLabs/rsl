# `bnb` examples

Runnable, self-checking walkthroughs — each ends in `all checks passed` (or panics). They're
grouped by what they exercise; the [guide module](https://docs.rs/bnb/latest/bnb/guide/) is the
prose companion. Run any with `cargo run -p bitsandbytes --example <name> [--features <feat>]`.

## Field types & macros (no codec)

| Example | Shows | Run |
|---|---|---|
| `standalone` | `#[bitfield]` + `#[derive(BitEnum)]` packed/unpacked directly — the dependency-light path | `--example standalone` |
| `enums` | `#[derive(BitEnum)]` in depth: exhaustive, `#[catch_all]` (the `num_enum` pattern), nesting, checked-int errors | `--example enums` |
| `flags` | `#[bitflags]` in depth: set algebra, per-flag accessors, iteration, **retain-vs-truncate** of unknown bits, nesting in `#[bin]` | `--example flags` |

## The `#[bin]` whole-message codec

| Example | Shows | Run |
|---|---|---|
| `ipv4` | An IPv4 header: nested `#[bitfield]`s, `map`, **verbatim vs canonical** encode (`encode_mode`/`to_canonical_bytes`) | `--example ipv4` |
| `bin_message` | The `#[bin]` fold end-to-end: bitfield + enum fields, `count`, `temp`/`calc`, `validate` | `--example bin_message` |
| `telemetry` | A telemetry frame: `#[bitflags]`, `#[reserved]`, `count`, `validate`, canonical encode | `--example telemetry` |
| `conditional` | `#[bin]` **`if`** — optional scalar + nested fields gated by a flag — plus `map` to a domain newtype | `--example conditional` |
| `versioned` | `#[bin]` **`if`** gated by a *version* field (v1 vs v2 layout), with a `try_map` version guard | `--example versioned` |
| `ctx` | **`ctx`** context threading + enum **`tag`** dispatch: a body whose variant is chosen by an off-wire selector | `--example ctx` |
| `ctx_length` | **`ctx`** sizing a field: a column count threaded into a `count`-loop of rows (`decode_with`/`…Ctx`) | `--example ctx_length` |
| `tlv` | A Type-Length-Value codec: enum `magic` dispatch over `count`-driven heterogeneous records | `--example tlv` |
| `checked` | **`try_map`** — reject an unrepresentable wire value at decode (`ErrorKind::Convert`, with field + bit offset) | `--example checked` |
| `varint` | **`parse_with`/`write_with`** — a custom LEB128 variable-length integer field codec | `--example varint` |
| `dns` | **Flagship** — a DNS message: `parse_with`, name compression via seeking, `count`/`#[nested]` sections, enum dispatch, UDP loopback | `--example dns` |

## I/O ladder & transports

| Example | Shows | Run |
|---|---|---|
| `archive` | `SeekReader` random access: a container index of `(offset, length)` records seeked to **out of order** | `--example archive` |
| `peek` | `SeekReader` + **`restore_position`** — read a discriminant, then rewind so a later field re-reads it | `--example peek` |
| `framed` | The opt-in `bytes` adapters (`BytesReader`/`BytesWriter`) + the streaming `Incomplete` signal | `--example framed --features bytes` |
| `tcp` | Raw `std` TCP: `BufSource` + the `&TcpStream` duplex trick (read + write one socket, no `try_clone`) | `--example tcp` |
| `sockets` | The `net` feature: `MessageStream` (TCP) and `MessageDatagram` (UDP **and** Unix datagram — one API) | `--example sockets --features net` |
| `tokio_framed` | The `tokio` feature: `BinCodec` over an async `Framed` TCP stream | `--example tokio_framed --features tokio` |
| `tokio_udp` | The `tokio` feature: the *same* `BinCodec` over `UdpFramed` (async UDP datagrams) | `--example tokio_udp --features tokio` |

## Feature → example coverage

| Feature / capability | Examples |
|---|---|
| `#[bitfield]` | standalone, ipv4, enums, dns, telemetry, bin_message |
| `#[derive(BitEnum)]` | enums, standalone, ipv4, dns, telemetry, bin_message |
| `#[bitflags]` | flags, telemetry |
| `#[bin]` magic dispatch | tlv, dns, framed, tcp, sockets, tokio_* |
| `count` / `#[nested]` | tlv, dns, telemetry, bin_message, archive, framed |
| `temp`/`calc` | most `#[bin]` examples |
| `map` | conditional, ipv4 |
| `try_map` | checked, versioned |
| `if` (conditional) | conditional, versioned |
| `ctx` + `tag` dispatch | ctx, ctx_length |
| `parse_with` / `write_with` | varint, dns |
| seeking (`restore_position`) | archive, peek, dns |
| verbatim vs canonical | ipv4, telemetry |
| `validate` | bin_message, telemetry |
| I/O: `BufSource` / `SeekReader` / `StreamBitReader` | tcp / archive, peek / framed |
| `bytes` / `tokio` / `net` features | framed / tokio_framed + tokio_udp / sockets |
