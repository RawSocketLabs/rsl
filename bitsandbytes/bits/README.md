# bits

Ergonomic, fast bit/byte field types and macros for binary protocol codecs,
with first-class [`binrw`](https://docs.rs/binrw) integration.

`bits` exists to retire a stack of overlapping helpers — `modular-bitfield`,
`modular-bitfield-msb`, `bitfield-struct`, `bitbybit`, `arbitrary-int`,
`num_enum` — behind one crate that is:

- **Integer-backed and fast** — fields are plain shift/mask on a single backing
  integer (no `bitvec`).
- **Explicit about ordering** — independent control of **bit order** (MSB/LSB
  first) and **byte order** (big/little), which is exactly what protocol layouts
  need.
- **Native to binrw** — `#[bitfield]` and `#[derive(BitEnum)]` types implement
  `BinRead`/`BinWrite`, so they drop into a `#[binrw]` struct with **no
  `#[br(map)]` / `#[bw(map)]` glue**.

## What's in it

| Item | Replaces | Purpose |
|---|---|---|
| `u1`..`u127` (`UInt<T, N>`) | `arbitrary-int` | sub-byte unsigned integers |
| `#[bitfield]` | `modular-bitfield(-msb)`, `bitbybit`, `bitfield-struct` | pack typed fields into one integer |
| `#[derive(BitEnum)]` | `num_enum`, `bitbybit::bitenum` | enum ⇄ integer with optional catch-all |

## Example

```rust
use bits::{bitfield, u4, BitEnum};

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
#[bitfield(u16, bits = msb, bytes = be)]
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

With the default `binrw` feature, `State` embeds in a `#[binrw]` struct directly:

```rust,ignore
#[binrw]
#[brw(big)]
struct Header {
    id: u16,
    state: State,   // no map glue
    qdcount: u16,
}
```

## Bit order vs. byte order

- `bits = msb | lsb` (default `msb`): does the **first** declared field land in
  the high or low bits.
- `bytes = be | le` (default `be`): byte order of the backing integer.

A bitfield's declared byte order is **intrinsic** — it wins over the surrounding
`#[binrw]` struct's endianness, because a protocol field's byte order is a
property of the field, not its context.

## Field widths

In order of precedence: an explicit `#[bits(N)]`; an explicit `#[bits(A..=B)]`
range (fixing the absolute offset — the manual escape hatch); or, by default,
`<FieldType as bits::Bits>::BITS`. Use inference/widths for automatic layout, or
ranges on every field for fully manual layout — the two styles cannot be mixed.

## Features

- `binrw` (default on): emit `BinRead`/`BinWrite` impls and re-export `binrw`.
  Turn it off for a standalone, dependency-light bit/byte library.

## Crate split

`bits` (this crate, the runtime types) re-exports the macros from `bits-macros`
(the proc-macro crate). Depend only on `bits`.

See [`DESIGN.md`](DESIGN.md) for the full rationale and roadmap.
