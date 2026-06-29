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
| `bitfield_bytes` | `#[bitfield]` **byte order**: the declared `bytes = be\|le` drives `to_bytes()`/`from_bytes()`; `to_be_bytes`/`to_le_bytes` are the explicit override | `--example bitfield_bytes` |

## The `#[bin]` whole-message codec

| Example | Shows | Run |
|---|---|---|
| `ipv4` | An IPv4 header: nested `#[bitfield]`s, `map`, **verbatim vs canonical** encode (`encode_mode`/`to_canonical_bytes`) | `--example ipv4` |
| `bin_message` | The `#[bin]` fold end-to-end: bitfield + enum fields, `count`, `temp`/`calc`, `validate` | `--example bin_message` |
| `arbitrary_width` | A 48-bit `#[derive(BitEnum)]` (a long sync/magic word) in a **non-byte-aligned 54-bit** `#[bin]` message; `#[catch_all]` keeps unknown syncs | `--example arbitrary_width` |
| `ais` | A real bit-packed format — an AIS marine Position Report: `u6`/`u2`/`u30`/`u4`/`u10` fields, **52-bit** non-byte-aligned total, `#[catch_all]` nav-status | `--example ais` |
| `can_signals` | The same, **LSB-first** (`bit_order = lsb`, CAN/DBC "Intel" signals): `u3`/`u4`/`u14`/`bool` packed low-bit-first into a non-byte-aligned 22-bit frame | `--example can_signals` |
| `endianness` | **Bit order × byte order are independent knobs**: the `#[bin]` 2×2 (`msb`/`lsb` × `big`/`little`) all distinct + round-trip, plus the low-level `BitReader`/`BitWriter` explicit `Layout` | `--example endianness` |
| `wav` | **Little-endian** byte order (`#[bin(little)]`): a RIFF/WAVE `fmt ` chunk of multi-byte LE integers, with `#[try_str]` on the `[u8; 4]` tag | `--example wav` |
| `telemetry` | A telemetry frame: `#[bitflags]`, `#[reserved]`, `count`, `validate`, canonical encode | `--example telemetry` |
| `reserved` | `#[reserved]` + the **verbatim vs canonical** model: `to_canonical_bytes`, `is_canonical`/`canonical_diff`, value-carried `encode_mode` | `--example reserved` |
| `alignment` | `pad_before` + `align_after` positioning with typed amounts (`4.bits()`, `1.bytes()`) | `--example alignment` |
| `padding` | `align_before` + `pad_after` — realign after a sub-byte field, fixed-size trailing pad | `--example padding` |
| `register` | `#[reserved]`/`#[reserved_with]` (must-be-zero + must-be-one) **and** `pad` in a fixed-layout control register | `--example register` |
| `conditional` | `#[bin]` **`if`** — optional scalar + nested fields gated by a flag — plus `map` to a domain newtype | `--example conditional` |
| `versioned` | `#[bin]` **`if`** gated by a *version* field (v1 vs v2 layout), with a `try_map` version guard | `--example versioned` |
| `heartbeat` | three at once — `#[bitflags]` status + `map` to a typed voltage + `if` gated by a **flag** | `--example heartbeat` |
| `ctx` | **`ctx`** context threading + enum **`tag`** dispatch: a body whose variant is chosen by an off-wire selector | `--example ctx` |
| `ctx_length` | **`ctx`** sizing a field: a column count threaded into a `count`-loop of rows (`decode_with`/`…Ctx`) | `--example ctx_length` |
| `versioned_cells` | **`ctx`** + **`try_map`**: a `try_map`-validated version threaded into each cell to set its data width | `--example versioned_cells` |
| `tlv` | A Type-Length-Value codec: enum `magic` dispatch over `count`-driven heterogeneous records | `--example tlv` |
| `checked` | **`try_map`** — reject an unrepresentable wire value at decode (`ErrorKind::Convert`, with field + bit offset) | `--example checked` |
| `wire_map` | **Struct-level wire mapping** (`#[bin(wire = W)]`): a logical type serialized via a separate wire type through `From`/`From<&Self>` impls — reusable in-program, nests via a one-line `FixedBitLen` | `--example wire_map` |
| `wire_map_dynamic` | The other wire-mapping forms: a **variable-length** wire (`String` over a length prefix), the inline **closure** form (`map`/`bw_map`), and the fallible **`try_wire`** (`TryFrom`) | `--example wire_map_dynamic` |
| `varint` | **`parse_with`/`write_with`** — a custom LEB128 variable-length integer field codec | `--example varint` |
| `cstring` | **`parse_with`/`write_with`** — a NUL-terminated C string (a third custom-codec shape) | `--example cstring` |
| `validate` | **`validate`** — a `build()`-gating predicate + re-runnable `is_valid()`; the parser stays permissive | `--example validate` |
| `try_str` | **`#[try_str]`** — a `Debug` hint: a byte buffer prints as a string when valid UTF-8, else hex bytes (never lossy) | `--example try_str` |
| `dns` | **Flagship** — a DNS message: `parse_with`, name compression via seeking, `count`-driven sections, enum dispatch, UDP loopback | `--example dns` |

## I/O ladder & transports

| Example | Shows | Run |
|---|---|---|
| `archive` | `SeekReader` random access: a container index of `(offset, length)` records seeked to **out of order** | `--example archive` |
| `peek` | `SeekReader` + **`restore_position`** — read a discriminant, then rewind so a later field re-reads it | `--example peek` |
| `streaming` | `StreamBitReader` — decode a *sequence* of messages off a forward-only stream; clean stop on `Incomplete` | `--example streaming` |
| `bufsource` | `BufSource` retain-and-seek — a backward `restore_position` over a reader that **can't** seek (the socket+seek case) | `--example bufsource` |
| `bitbuf` | `BitBuf` push/pull — feed chunks as they arrive, pull whole messages; compared against `decode_all`/`BitReader`/`BufSource` on the same buffer | `--example bitbuf` |
| `bitbuf_bounded` | `BitBuf::bounded(cap)` — **alloc-once** fixed capacity: `try_push` (refuses to grow → `CapacityError`), deferred **in-place reclaim**, explicit `grow` | `--example bitbuf_bounded` |
| `framed` | The opt-in `bytes` adapters (`BytesReader`/`BytesWriter`) + the streaming `Incomplete` signal | `--example framed --features bytes` |
| `bytes_frame` | The `bytes` feature: zero-copy framing — encode to a `Bytes`, decode from an owned `Bytes`, cheap slices | `--example bytes_frame --features bytes` |
| `tcp` | Raw `std` TCP: `BufSource` + the `&TcpStream` duplex trick (read + write one socket, no `try_clone`) | `--example tcp` |
| `sockets` | The `net` feature: `MessageStream` (TCP) and `MessageDatagram` (UDP **and** Unix datagram — one API) | `--example sockets --features net` |
| `unix_stream` | The `net` feature: `MessageStream` over a Unix-domain **stream** socket — generic beyond `TcpStream` | `--example unix_stream --features net` |
| `mock_datagram` | The `mock` feature: unit-test `MessageDatagram` code with `MockDatagramSocket` (no real socket); the sealed-trait + generic-handler pattern | `--example mock_datagram --features mock` |
| `mock_stream` | The `mock` feature: unit-test `MessageStream` code with `MockStream` (a `Read + Write`; **1 byte/read** drives the buffer-more-and-retry framing path `Cursor` can't) | `--example mock_stream --features mock` |
| `tokio_framed` | The `tokio` feature: `BinCodec` over an async `Framed` TCP stream | `--example tokio_framed --features tokio` |
| `tokio_udp` | The `tokio` feature: the *same* `BinCodec` over `UdpFramed` (async UDP datagrams) | `--example tokio_udp --features tokio` |

## Feature → example coverage

| Feature / capability | Examples |
|---|---|
| `#[bitfield]` | standalone, ipv4, enums, dns, telemetry, bin_message |
| `#[derive(BitEnum)]` | enums, standalone, ipv4, dns, telemetry, bin_message, arbitrary_width, ais, can_signals |
| arbitrary bit widths / non-byte-aligned message | arbitrary_width, ais, can_signals |
| byte/bit order (`little` / `lsb`, the 2×2 + low-level `Layout`) | wav, can_signals, endianness |
| `#[bitfield]` byte order (`to_bytes`/`from_bytes`) | bitfield_bytes |
| `#[bitflags]` | flags, telemetry, heartbeat |
| `#[bin]` magic dispatch | tlv, dns, framed, tcp, sockets, tokio_* |
| `count` (`Vec` of leaves or messages — no marker) | tlv, dns, telemetry, bin_message, archive, framed |
| `temp`/`calc` | most `#[bin]` examples |
| `map` | conditional, ipv4, heartbeat |
| `try_map` | checked, versioned, versioned_cells |
| struct-level wire mapping (`wire`/`try_wire`, struct `map`) | wire_map, wire_map_dynamic |
| `if` (conditional) | conditional, versioned, heartbeat |
| `ctx` + `tag` dispatch | ctx, ctx_length, versioned_cells |
| `parse_with` / `write_with` | varint, cstring, dns |
| seeking (`restore_position`) | archive, peek, bufsource, dns |
| `pad` / `align` | alignment, padding, register |
| `#[reserved]` / `#[reserved_with]` | reserved, register, telemetry |
| verbatim vs canonical (`encode_mode`) | reserved, ipv4, telemetry |
| `validate` | validate, bin_message, telemetry |
| `#[try_str]` (Debug rendering) | try_str, checked, ctx, wav |
| I/O: `BufSource` / `SeekReader` / `StreamBitReader` / `BitBuf` | tcp, bufsource / archive, peek / framed, streaming / bitbuf, bitbuf_bounded |
| `BitBuf` bounded (alloc-once `try_push`/`grow`/`CapacityError`) | bitbuf_bounded |
| `bytes` feature (zero-copy) | framed, bytes_frame |
| `tokio` feature | tokio_framed, tokio_udp |
| `net` feature | sockets, unix_stream |
| `mock` feature (test net code) | mock_datagram, mock_stream |
