# bnb

[![crates.io](https://img.shields.io/crates/v/bitsandbytes.svg)](https://crates.io/crates/bitsandbytes)
[![docs.rs](https://docs.rs/bitsandbytes/badge.svg)](https://docs.rs/bitsandbytes)

An **owned, bit-aware binary codec**: ergonomic, fast bit/byte field types plus a
unified `#[bin]` whole-message macro for binary protocols. No external codec
dependency — `bnb` is self-contained.

Published on crates.io as **`bitsandbytes`**; import it as `bnb`:
`bnb = { package = "bitsandbytes", version = "0.8" }`. Docs: <https://docs.rs/bitsandbytes>.
What changed in each release is in [`CHANGELOG.md`](CHANGELOG.md).

`bnb` collapses a stack of overlapping helpers — `modular-bitfield(-msb)`,
`bitfield-struct`, `bitbybit`, `arbitrary-int`, `num_enum`, and a `binrw`-style
codec — into one crate that is:

- **Integer-backed and fast** — bitfields are plain shift/mask on a single backing
  integer (no `bitvec`); the stream codec reads/writes at arbitrary **bit** offsets.
- **Explicit about ordering** — independent control of **bit order** (MSB/LSB-first)
  and **byte order** (big/little), exactly what protocol layouts need.
- **Dual-use by default** — the guided path is RFC-correct, but parsers stay
  permissive (unknown enum values are preserved as a catch-all, never rejected), so
  fuzzing / red-teaming / interop testing can emit deliberately non-conformant data.

## What's in it

| Item | Replaces | Purpose |
|---|---|---|
| `u1`..`u127` (`UInt<T, N>`) | `arbitrary-int` | sub-byte unsigned integers |
| `#[bitfield]` | `modular-bitfield(-msb)`, `bitbybit`, `bitfield-struct` | pack typed fields into one integer |
| `#[derive(BitEnum)]` | `num_enum`, `bitbybit::bitenum` | enum ⇄ integer with an optional catch-all |
| `#[bitflags]` | `bitflags` | named single-bit flag sets with set algebra |
| `#[derive(BitsBuilder)]` | `derive_builder` (for bit/byte structs) | required-by-default builder |
| **`#[bin]`** | a hand-written codec | **whole-message codec**: magic/count/ctx/map/if/calc/reserved/positioning/validate |

## Example

Not compiled here; the tested version is the crate-level example on
[docs.rs](https://docs.rs/bitsandbytes).

```rust,ignore
use bnb::{bitfield, BitEnum, u4};

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    #[catch_all]
    Other(u4), // unknown values preserved (dual-use)
}

// MSB-first packing (network order), big-endian on the wire.
#[bitfield(u16, bits = msb, bytes = big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: u4,
    flags:  u8,
    rcode:  RCode,
}

let s = State::new()
    .with_opcode(u4::new(2))
    .with_rcode(RCode::ServFail);

assert_eq!(s.to_be_bytes(), [0x20, 0x02]);
assert_eq!(s.rcode(), RCode::ServFail);
```

More examples — flags, `#[bin]` messages, the I/O ladder — are in the doctested
[guide](https://docs.rs/bitsandbytes/latest/bnb/guide/) and [`examples/`](examples/README.md).

## Bit order vs. byte order

- `bits = msb | lsb` (default `msb`): does the **first** declared field land in the
  high or low bits of the backing integer.
- `bytes = big | little` (default `big`): byte order of the backing integer on the wire.

These are independent knobs — the whole point. MSB-first big-endian matches the
ASCII-art layouts in RFCs.

## Field widths

In order of precedence: an explicit `#[bits(N)]`; an explicit `#[bits(A..=B)]`
range (which fixes the absolute offset — the manual layout escape hatch); or, by
default, `<FieldType as bnb::Bits>::BITS`. Use inference/widths for automatic
layout, or ranges on every field for fully manual layout — the two styles cannot be
mixed in one struct.

## Crate layout

`bnb` (this crate, the runtime types) re-exports the macros from `bnb-macros` (the
proc-macro crate). Depend only on `bnb`.

| Feature | Adds |
|---|---|
| `std` *(default)* | the `std::io` reader/writer ladder |
| `bytes` | zero-copy `bytes`-crate adapters |
| `net` | `MessageStream` / `MessageDatagram` socket helpers |
| `tokio-io` | borrowed `net::read_message_async` over Tokio `AsyncRead` |
| `tokio` | `BinCodec` for `tokio_util` framing (implies `bytes`, `tokio-io`) |
| `mock` | in-memory test transports for `net` code |

Details: [Feature flags](https://docs.rs/bitsandbytes/latest/bnb/#feature-flags--no_std).
Credits to the crates that shaped `bnb` are in [`ACKNOWLEDGMENTS.md`](ACKNOWLEDGMENTS.md).

See [`DESIGN.md`](DESIGN.md) for the design rationale and [`ROADMAP.md`](ROADMAP.md)
for the capability/status summary.
